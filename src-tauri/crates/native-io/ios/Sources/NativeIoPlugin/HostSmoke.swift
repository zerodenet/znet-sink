import Foundation
import WebKit

// Not a product success signal. Only Debug plus an explicit per-launch nonce
// can evaluate the fixed, test-bundle driver. Release has no driver entry point.
enum HostSmoke {
    enum Stage: String {
        case pluginInitialized = "plugin-initialized"
        case webviewAttached = "webview-attached"
        case waitingForDocument = "waiting-for-document"
        case evaluatingDriver = "evaluating-driver"
        case javascriptReturned = "javascript-returned"
    }
    /// Separate progress evidence, never a completed UI/IPC or VPN receipt.
    static func note(_ stage: Stage, webview: WKWebView? = nil) {
        #if DEBUG
        guard let nonce = ProcessInfo.processInfo.environment["ZNET_IOS_UI_SMOKE_NONCE"],
              nonce.count == 32, nonce.allSatisfy({ "0123456789abcdef".contains($0) }),
              let directory = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask).first else { return }
        var value: [String: Any] = ["schemaVersion": 1, "nonce": nonce, "stage": stage.rawValue,
                                   "pid": ProcessInfo.processInfo.processIdentifier,
                                   "bundleIdentifier": Bundle.main.bundleIdentifier ?? ""]
        if let view = webview {
            value["documentScheme"] = ["tauri", "http", "https", "about"].contains(view.url?.scheme ?? "")
                ? view.url!.scheme! : "absent-or-other"
            value["documentIsLoading"] = view.isLoading
        }
        diagnostic("startup stage: " + stage.rawValue)
        guard let bytes = try? JSONSerialization.data(withJSONObject: value, options: [.sortedKeys]),
              bytes.count < 4096 else { return }
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        try? bytes.write(to: directory.appendingPathComponent("znet-ios-startup-smoke.json"), options: .atomic)
        #endif
    }
    static func start(_ webview: WKWebView) {
        #if DEBUG
        guard let nonce = ProcessInfo.processInfo.environment["ZNET_IOS_UI_SMOKE_NONCE"],
              nonce.count == 32, nonce.allSatisfy({ "0123456789abcdef".contains($0) }) else { return }
        diagnostic("native plugin accepted explicit launch nonce")
        note(.webviewAttached, webview: webview)
        guard let scriptURL = Bundle.main.url(forResource: "ios-host-smoke", withExtension: "js"),
              let script = try? String(contentsOf: scriptURL, encoding: .utf8),
              script.utf8.count < 32768 else {
            persist(nil, completed: false, webview: webview, nonce: nonce, stage: "driver-unavailable")
            return
        }
        waitForDocument(webview, script: script, nonce: nonce, remaining: 120)
        #endif
    }

    #if DEBUG
    private static func waitForDocument(_ webview: WKWebView, script: String, nonce: String, remaining: Int) {
        if remaining == 120 { note(.waitingForDocument, webview: webview) }
        // Plugin load precedes navigation. A fixed delay can evaluate in the
        // initial document and lose its async frame when the app URL loads.
        if webview.url?.scheme == "tauri", !webview.isLoading {
            evaluate(webview, script: script, nonce: nonce)
            return
        }
        guard remaining > 0 else {
            persist(nil, completed: false, webview: webview, nonce: nonce, stage: "document-readiness-timeout")
            return
        }
        let work = DispatchWorkItem { [weak webview] in
            guard let view = webview else { return }
            waitForDocument(view, script: script, nonce: nonce, remaining: remaining - 1)
        }
        DispatchQueue.main.asyncAfter(deadline: DispatchTime.now() + .milliseconds(500), execute: work)
    }

    private static func evaluate(_ webview: WKWebView, script: String, nonce: String) {
        note(.evaluatingDriver, webview: webview)
        diagnostic("evaluating fixed driver in actual WKWebView")
        webview.callAsyncJavaScript(script, arguments: [:], in: nil, in: .page) { result in
            switch result {
            case .success(let value):
                persist(value, completed: true, webview: webview, nonce: nonce)
            case .failure(let error):
                persist(nil, completed: false, webview: webview, nonce: nonce, error: error as NSError)
            }
        }
    }

    private static func diagnostic(_ message: String) {
        // Unbuffered fixed test messages survive a failed launch. Never include
        // navigation URLs, user configuration, WebKit exception text or secrets.
        FileHandle.standardError.write(Data("[ZNet iOS smoke] \(message)\n".utf8))
    }

    private static func persist(_ checks: Any?, completed: Bool, webview: WKWebView, nonce: String,
                                error: NSError? = nil, stage: String = "javascript-callback") {
        if stage == "javascript-callback" { note(.javascriptReturned, webview: webview) }
        diagnostic(stage)
        var inspectable = false
        if #available(iOS 16.4, *) { inspectable = webview.isInspectable }
        var receipt: [String: Any] = [
            "schemaVersion": 1, "nonce": nonce,
            "bundleIdentifier": Bundle.main.bundleIdentifier ?? "",
            "minimumSystemVersion": Bundle.main.object(forInfoDictionaryKey: "MinimumOSVersion") ?? "",
            "nativePlugin": "znet-native-io", "webViewInspectable": inspectable,
            "status": completed ? "completed" : "failed", "stage": stage,
            "documentScheme": ["tauri", "http", "https", "about"].contains(webview.url?.scheme ?? "")
                ? webview.url!.scheme! : "absent-or-other",
            "documentHostIsLocalhost": webview.url?.host == "localhost",
            "documentIsLoading": webview.isLoading
        ]
        if let checks = checks { receipt["checks"] = checks }
        if !completed { receipt["reason"] = "actual-webview-smoke-failed" }
        if let error = error {
            receipt["errorDomain"] = error.domain == "WKErrorDomain" ? "WKErrorDomain" : "native-webview-error"
            receipt["errorCode"] = error.code
        }
        // Fixed private cache output; no arbitrary path or user material.
        guard let directory = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask).first,
              let bytes = try? JSONSerialization.data(withJSONObject: receipt, options: [.sortedKeys]),
              bytes.count < 65536 else { return }
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        try? bytes.write(to: directory.appendingPathComponent("znet-ios-ui-smoke.json"), options: .atomic)
    }
    #endif
}
