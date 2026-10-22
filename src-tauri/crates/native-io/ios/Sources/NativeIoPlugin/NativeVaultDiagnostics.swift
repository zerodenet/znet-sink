#if DEBUG
import Foundation
import NativeIoPolicy
import Security

extension NativeVault {
    /// Separate account from both `master` and the normal vault round-trip smoke.
    /// A failing metadata probe does not suppress diagnosis of the isolated data
    /// query. This result never changes the production admission decision.
    static func inspectSmoke(nonce: String, protectedDataAvailable: Bool) -> NativeVaultSmokeDiagnostics {
        var result = NativeVaultSmokeDiagnostics(protectedDataAvailable: protectedDataAvailable,
                                                 admission: lastAdmission())
        // The explicit Debug nonce permits diagnosis of this temporary account
        // even when the production gate is closed. No production query, key or
        // admission is substituted; Security still enforces protection policy.
        result.stopStage = .scope
        guard let selection = diagnosticSelection(nonce) else { return result }
        result.availabilityProbe = NativeVaultStep(probeStatus(selection))
        func read() -> (OSStatus, Data?) {
            var request = selection
            request[kSecReturnData as String] = true
            request[kSecMatchLimit as String] = kSecMatchLimitOne
            var value: CFTypeRef?
            let status = SecItemCopyMatching(request as CFDictionary, &value)
            return (status, value as? Data)
        }
        func finish(_ stage: NativeVaultStopStage, cleanup: Bool) -> NativeVaultSmokeDiagnostics {
            result.stopStage = stage
            if cleanup {
                let status = SecItemDelete(selection as CFDictionary)
                result.cleanup = NativeVaultStep(status)
                if stage == .completed && status != errSecSuccess && status != errSecItemNotFound {
                    result.stopStage = .cleanup
                }
            }
            result.roundTrip = result.stopStage == .completed
            return result
        }
        var before = read()
        defer {
            if let count = before.1?.count { before.1?.resetBytes(in: 0..<count) }
        }
        result.readBeforeCreate = NativeVaultStep(before.0)
        // Never overwrite a preexisting diagnostic account, including after a
        // lost earlier response. Cleanup remains limited to this smoke nonce.
        guard before.0 == errSecItemNotFound else {
            return finish(.readBeforeCreate, cleanup: before.0 == errSecSuccess)
        }
        var key = Data(count: 32)
        defer { key.resetBytes(in: 0..<key.count) }
        let random = key.withUnsafeMutableBytes { bytes in
            SecRandomCopyBytes(kSecRandomDefault, bytes.count, bytes.baseAddress!)
        }
        result.random = NativeVaultStep(random)
        guard random == errSecSuccess else { return finish(.random, cleanup: false) }
        var insertion = selection
        insertion[kSecUseAuthenticationUI as String] = nil
        insertion[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        insertion[kSecValueData as String] = key
        let created = SecItemAdd(insertion as CFDictionary, nil)
        result.create = NativeVaultStep(created)
        guard created == errSecSuccess else { return finish(.create, cleanup: true) }
        var after = read()
        defer {
            if let count = after.1?.count { after.1?.resetBytes(in: 0..<count) }
        }
        result.readAfterCreate = NativeVaultStep(after.0)
        if after.0 == errSecSuccess {
            result.readBackMatches = after.1?.count == 32 && after.1 == key
        }
        guard result.readBackMatches == true else { return finish(.readAfterCreate, cleanup: true) }
        return finish(.completed, cleanup: true)
    }
}
#endif
