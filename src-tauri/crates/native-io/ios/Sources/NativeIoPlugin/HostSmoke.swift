import Foundation
import WebKit

// Not a product success signal. Only Debug plus an explicit per-launch nonce
// can evaluate the fixed, test-bundle driver. Release has no driver entry point.
enum HostSmoke {
    static func start(_ webview: WKWebView) {
        #if DEBUG
        guard let nonce = ProcessInfo.processInfo.environment["ZNET_IOS_UI_SMOKE_NONCE"],
              nonce.count == 32, nonce.allSatisfy({ "0123456789abcdef".contains($0) }),
              let scriptURL = Bundle.main.url(forResource: "ios-host-smoke", withExtension: "js"),
              let script = try? String(contentsOf: scriptURL, encoding: .utf8),
              script.utf8.count < 32768 else { return }
        DispatchQueue.main.asyncAfter(deadline: .now() + 3) { [weak webview] in
            guard let webview = webview else { return }
            webview.callAsyncJavaScript(script, arguments: [:], in: nil, contentWorld: .page) { result in
                var inspectable = false
                if #available(iOS 16.4, *) { inspectable = webview.isInspectable }
                var receipt: [String: Any] = [
                    "schemaVersion": 1, "nonce": nonce,
                    "bundleIdentifier": Bundle.main.bundleIdentifier ?? "",
                    "minimumSystemVersion": Bundle.main.object(forInfoDictionaryKey: "MinimumOSVersion") ?? "",
                    "nativePlugin": "znet-native-io", "webViewInspectable": inspectable
                ]
                switch result {
                case .success(let value):
                    receipt["status"] = "completed"
                    receipt["checks"] = value
                case .failure:
                    receipt["status"] = "failed"
                    receipt["reason"] = "actual-webview-smoke-failed"
                }
                // Fixed private cache output; no arbitrary path or user material.
                guard let directory = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask).first,
                      let bytes = try? JSONSerialization.data(withJSONObject: receipt, options: [.sortedKeys]),
                      bytes.count < 65536 else { return }
                try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
                try? bytes.write(to: directory.appendingPathComponent("znet-ios-ui-smoke.json"), options: .atomic)
            }
        }
        #endif
    }
}
