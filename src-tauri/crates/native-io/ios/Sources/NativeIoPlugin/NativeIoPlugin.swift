import CryptoKit
import Foundation
import NativeIoPolicy
import Tauri
import UIKit
import UniformTypeIdentifiers
import WebKit

private final class NativeOperation: NSObject, UIDocumentPickerDelegate,
    UIAdaptivePresentationControllerDelegate {
    let request: NativeIoRequest
    let action: NativeIoAction
    let invoke: Invoke
    weak var owner: NativeIoPlugin?
    weak var controller: UIViewController?
    var lease: NativeFileLease?
    var expectedDigest: Data?
    var expectedBytes: Int?
    var presented = false
    var processing = false
    var finished = false

    init(request: NativeIoRequest, action: NativeIoAction, invoke: Invoke,
         owner: NativeIoPlugin) {
        self.request = request
        self.action = action
        self.invoke = invoke
        self.owner = owner
    }

    func documentPicker(_ controller: UIDocumentPickerViewController,
                        didPickDocumentsAt urls: [URL]) {
        guard owner?.isCurrent(self) == true, !processing else { return }
        guard urls.count == 1, let url = urls.first, url.isFileURL else {
            owner?.finish(self, status: action == .save ? "result-unknown" : "failed",
                          reason: "invalid-document-result")
            return
        }
        processing = true
        owner?.consumeDocument(self, url: url)
    }

    func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
        guard !processing else { return }
        owner?.finish(self, status: "cancelled")
    }

    func presentationControllerDidDismiss(_ presentationController: UIPresentationController) {
        guard !processing else { return }
        owner?.finish(self, status: "cancelled")
    }
}

/// UIKit owns file authorization and interaction; Rust owns preparation and business persistence.
/// No paths, URLs, bookmarks, or protected material are returned across the bridge.
final class NativeIoPlugin: Plugin {
    private var active: NativeOperation?
    private var foregroundObserver: NSObjectProtocol?
    private let io = DispatchQueue(label: "org.zerodenet.znetsink.native-io", qos: .userInitiated)
    private let stageRoot = FileManager.default.temporaryDirectory
        .appendingPathComponent("znet-native-io-leases", isDirectory: true)

    override func load(webview: WKWebView) {
        super.load(webview: webview)
        HostSmoke.start(webview)
    }

    override init() {
        super.init()
        // Startup cleanup is ordered before stage creation on the same queue. It removes only
        // this adapter's lease root, never Documents, profiles, subscription caches or logs.
        let root = stageRoot
        io.async { try? FileManager.default.removeItem(at: root) }
        foregroundObserver = NotificationCenter.default.addObserver(
            forName: UIApplication.didBecomeActiveNotification, object: nil, queue: .main
        ) { [weak self] _ in self?.reconcilePresentation() }
    }

    deinit {
        if let observer = foregroundObserver { NotificationCenter.default.removeObserver(observer) }
    }

    @objc public func pickRead(_ invoke: Invoke) { begin(invoke, action: .pickRead) }
    @objc public func save(_ invoke: Invoke) { begin(invoke, action: .save) }
    @objc public func share(_ invoke: Invoke) { begin(invoke, action: .share) }

    /// A fixed application resource; no user path, document grant or permission prompt.
    @objc public func readLicenseInventory(_ invoke: Invoke) {
        io.async {
            guard let url = Bundle.main.url(forResource: "license-inventory.native",
                                            withExtension: "json", subdirectory: "assets") else {
                invoke.resolve(["status": "missing", "reason": "license_asset_not_packaged"])
                return
            }
            do {
                // Reuse the tested chunked reader, including its witness byte
                // and short-read handling; metadata alone does not bound IO.
                let data = try BoundedNativeIo.read(url: url, maximumBytes: 16 * 1024 * 1024)
                guard let text = String(data: data, encoding: .utf8),
                      let value = try JSONSerialization.jsonObject(with: data) as? [String: Any],
                      value["schemaVersion"] as? Int == 1,
                      let target = value["target"] as? [String: Any],
                      target["platform"] as? String == "ios" else {
                    invoke.resolve(["status": "invalid", "reason": "license_asset_invalid"])
                    return
                }
                invoke.resolve(["status": "available", "content": text])
            } catch {
                invoke.resolve(["status": "invalid", "reason": "license_asset_unreadable"])
            }
        }
    }

    private func begin(_ invoke: Invoke, action: NativeIoAction) {
        // The wire payload is already resident; prevent another oversized decode/allocation.
        guard invoke.getRawArgs().utf8.count <= 24 * 1024 * 1024 else {
            invoke.reject("invalid-request", code: "invalid-request")
            return
        }
        let request: NativeIoRequest
        do {
            request = try invoke.parseArgs(NativeIoRequest.self)
        } catch {
            // Malformed input is rejected using a static code, never parser messages or content.
            invoke.reject("invalid-request", code: "invalid-request")
            return
        }
        do {
            try request.validate(action: action)
        } catch {
            let reason = (error as? NativeIoError)?.rawValue ?? "invalid-request"
            if (error as? NativeIoError) == .invalidOperationId {
                invoke.reject(reason, code: reason)
            } else {
                invoke.resolve(NativeIoResponse(operationId: request.operationId,
                                                status: "failed", reason: reason))
            }
            return
        }
        DispatchQueue.main.async { [weak self] in
            guard let self = self else {
                invoke.resolve(NativeIoResponse(operationId: request.operationId,
                                                status: "failed", reason: "host-unavailable"))
                return
            }
            guard self.active == nil else {
                invoke.resolve(NativeIoResponse(operationId: request.operationId,
                                                status: "failed", reason: "operation-busy"))
                return
            }
            guard UIApplication.shared.applicationState == .active,
                  let presenter = self.manager.viewController,
                  presenter.viewIfLoaded?.window != nil,
                  !presenter.isBeingDismissed, !presenter.isBeingPresented,
                  presenter.presentedViewController == nil else {
                invoke.resolve(NativeIoResponse(operationId: request.operationId,
                                                status: "failed", reason: "presentation-unavailable"))
                return
            }
            let operation = NativeOperation(request: request, action: action, invoke: invoke, owner: self)
            self.active = operation
            if action == .pickRead {
                let picker = UIDocumentPickerViewController(
                    forOpeningContentTypes: [.json, .plainText, .data], asCopy: false)
                picker.allowsMultipleSelection = false
                picker.delegate = operation
                self.present(picker, operation: operation)
                return
            }
            // Decode and file IO never block the main thread. The validated action/purpose and
            // encoded-size guard above run before decoding or presenting a system interaction.
            self.io.async {
                do {
                    let payload = try request.exportData(action: action)
                    let lease = try NativeFileLease(root: self.stageRoot,
                                                   fileName: request.fileName!, data: payload)
                    // Data protection is set before the staged document is handed to UIKit.
                    do {
                        try FileManager.default.setAttributes(
                            [.protectionKey: FileProtectionType.complete], ofItemAtPath: lease.file.path)
                    } catch {
                        lease.cleanup()
                        throw NativeIoError.stageFailed
                    }
                    let digest = Data(SHA256.hash(data: payload))
                    let count = payload.count
                    DispatchQueue.main.async {
                        guard self.isCurrent(operation) else { lease.cleanup(); return }
                        operation.lease = lease
                        operation.expectedDigest = digest
                        operation.expectedBytes = count
                        if action == .save {
                            let picker = UIDocumentPickerViewController(forExporting: [lease.file], asCopy: true)
                            picker.delegate = operation
                            self.present(picker, operation: operation)
                        } else {
                            let activity = UIActivityViewController(activityItems: [lease.file],
                                                                    applicationActivities: nil)
                            activity.completionWithItemsHandler = { [weak self, weak operation] _, completed, _, error in
                                DispatchQueue.main.async {
                                    guard let self = self, let operation = operation,
                                          self.isCurrent(operation) else { return }
                                    if error != nil {
                                        self.finish(operation, status: "failed", reason: "share-failed")
                                    } else {
                                        self.finish(operation, status: completed ? "share-accepted" : "cancelled")
                                    }
                                }
                            }
                            self.present(activity, operation: operation)
                        }
                    }
                } catch {
                    let reason = (error as? NativeIoError)?.rawValue ?? "stage-failed"
                    DispatchQueue.main.async { self.finish(operation, status: "failed", reason: reason) }
                }
            }
        }
    }

    private func present(_ controller: UIViewController, operation: NativeOperation) {
        guard isCurrent(operation), UIApplication.shared.applicationState == .active,
              let presenter = manager.viewController, presenter.viewIfLoaded?.window != nil,
              !presenter.isBeingDismissed, !presenter.isBeingPresented,
              presenter.presentedViewController == nil else {
            finish(operation, status: "failed", reason: "presentation-unavailable")
            return
        }
        operation.controller = controller
        // A popover anchor is mandatory for UIActivityViewController on iPad.
        if let popover = controller.popoverPresentationController {
            popover.sourceView = presenter.view
            popover.sourceRect = CGRect(x: presenter.view.bounds.midX, y: presenter.view.bounds.midY,
                                        width: 1, height: 1)
            popover.permittedArrowDirections = []
        }
        presenter.present(controller, animated: true) { [weak self, weak operation] in
            guard let self = self, let operation = operation, self.isCurrent(operation) else { return }
            operation.presented = true
        }
        guard controller.presentingViewController != nil else {
            finish(operation, status: "failed", reason: "presentation-unavailable")
            return
        }
        // Track acceptance immediately, even if an interrupted transition loses its completion.
        operation.presented = true
        controller.presentationController?.delegate = operation
    }

    fileprivate func isCurrent(_ operation: NativeOperation) -> Bool {
        // Object identity is independent of the caller's operationId. A reused string cannot
        // make an old UIKit delegate or background IO deliver into a newer operation.
        active === operation && !operation.finished
    }

    fileprivate func consumeDocument(_ operation: NativeOperation, url: URL) {
        io.async {
            let scoped = url.startAccessingSecurityScopedResource()
            defer { if scoped { url.stopAccessingSecurityScopedResource() } }
            var coordinateError: NSError?
            var result: Result<Data, Error>?
            let coordinator = NSFileCoordinator(filePresenter: nil)
            coordinator.coordinate(readingItemAt: url, options: [], error: &coordinateError) { coordinatedURL in
                result = Result {
                    try BoundedNativeIo.read(url: coordinatedURL,
                                             maximumBytes: operation.request.maximumBytes)
                }
            }
            let data: Data?
            var reason: String?
            if coordinateError != nil {
                data = nil
                reason = "document-read-unavailable"
            } else {
                switch result {
                case .success(let bytes): data = bytes
                case .failure(let error):
                    data = nil
                    reason = (error as? NativeIoError)?.rawValue ?? "read-failed"
                case nil:
                    data = nil
                    reason = "document-read-unavailable"
                }
            }
            // Compute response on the IO queue; only delivery/state changes use the main queue.
            let response: NativeIoResponse
            if let data = data {
                if operation.action == .pickRead {
                    response = NativeIoResponse(operationId: operation.request.operationId, status: "read",
                                                bytesRead: data.count, contentBase64: data.base64EncodedString())
                } else if operation.expectedBytes == data.count,
                          operation.expectedDigest == Data(SHA256.hash(data: data)) {
                    response = NativeIoResponse(operationId: operation.request.operationId, status: "saved")
                } else {
                    response = NativeIoResponse(operationId: operation.request.operationId,
                                                status: "result-unknown", reason: "export-verification-failed")
                }
            } else {
                response = NativeIoResponse(operationId: operation.request.operationId,
                                            status: operation.action == .save ? "result-unknown" : "failed",
                                            reason: reason)
            }
            DispatchQueue.main.async { self.finish(operation, response: response) }
        }
    }

    fileprivate func finish(_ operation: NativeOperation, status: String, reason: String? = nil) {
        finish(operation, response: NativeIoResponse(operationId: operation.request.operationId,
                                                    status: status, reason: reason))
    }

    private func finish(_ operation: NativeOperation, response: NativeIoResponse) {
        guard isCurrent(operation) else { return }
        operation.finished = true
        active = nil
        let lease = operation.lease
        operation.lease = nil
        io.async { lease?.cleanup() }
        operation.invoke.resolve(response)
    }

    private func reconcilePresentation() {
        guard let operation = active, operation.presented, !operation.processing,
              operation.controller?.presentingViewController == nil else { return }
        // Backgrounding does not imply cancellation: Files/share may continue in another app.
        // If UIKit lost the interaction without a callback, clear busy state with unknown result.
        finish(operation, status: "result-unknown", reason: "interaction-result-unavailable")
    }
}

@_cdecl("init_plugin_znet_native_io")
func initPluginZnetNativeIo() -> Plugin { NativeIoPlugin() }
