import Foundation
import WebKit

// Not a product success signal. Only Debug plus an explicit per-launch nonce
// can evaluate the fixed, test-bundle driver. Release has no driver entry point.
enum HostSmoke {
    static func start(_ webview: WKWebView) {
        #if DEBUG
        guard let nonce = ProcessInfo.processInfo.environment["ZNET_IOS_UI_SMOKE_NONCE"],
              nonce.count == 32, nonce.allSatisfy({ "0123456789abcdef".contains($0) }) else { return }
        print("[ZNet iOS smoke] native plugin accepted explicit launch nonce")
        guard let scriptURL = Bundle.main.url(forResource: "ios-host-smoke", withExtension: "js"),
              let script = try? String(contentsOf: scriptURL, encoding: .utf8),
              script.utf8.count < 32768 else {
            print("[ZNet iOS smoke] bounded test driver unavailable")
            return
        }
        waitForDocument(webview, script: script, nonce: nonce, remaining: 120)
        #endif
    }

    #if DEBUG
    private static func waitForDocument(_ webview: WKWebView, script: String, nonce: String, remaining: Int) {
        // Plugin load precedes navigation. A fixed delay can evaluate in the
        // initial document and lose its async frame when the app URL loads.
        if webview.url?.scheme == "tauri", !webview.isLoading {
            evaluate(webview, script: script, nonce: nonce)
            return
        }
        guard remaining > 0 else {
            persist(nil, completed: false, webview: webview, nonce: nonce)
            return
        }
        let work = DispatchWorkItem { [weak webview] in
            guard let view = webview else { return }
            waitForDocument(view, script: script, nonce: nonce, remaining: remaining - 1)
        }
        DispatchQueue.main.asyncAfter(deadline: DispatchTime.now() + .milliseconds(500), execute: work)
    }

    private static func evaluate(_ webview: WKWebView, script: String, nonce: String) {
        print("[ZNet iOS smoke] evaluating fixed driver in actual WKWebView")
        webview.callAsyncJavaScript(script, arguments: [:], in: nil, in: .page) { result in
            switch result {
            case .success(let value):
                persist(value, completed: true, webview: webview, nonce: nonce)
            case .failure(let error):
                persist(nil, completed: false, webview: webview, nonce: nonce, error: error as NSError)
            }
        }
    }

    private static func persist(_ checks: Any?, completed: Bool, webview: WKWebView, nonce: String, error: NSError? = nil) {
        print("[ZNet iOS smoke] actual WebKit callback received")
        var inspectable = false
        if #available(iOS 16.4, *) { inspectable = webview.isInspectable }
        var receipt: [String: Any] = [
            "schemaVersion": 1, "nonce": nonce,
            "bundleIdentifier": Bundle.main.bundleIdentifier ?? "",
            "minimumSystemVersion": Bundle.main.object(forInfoDictionaryKey: "MinimumOSVersion") ?? "",
            "nativePlugin": "znet-native-io", "webViewInspectable": inspectable,
            "status": completed ? "completed" : "failed"
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
