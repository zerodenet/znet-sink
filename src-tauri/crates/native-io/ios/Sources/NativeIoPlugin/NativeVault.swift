import Foundation
import Security
import NativeIoPolicy

/// ThisDeviceOnly, no iCloud sharing, no UI authentication prompt. No raw key file.
enum NativeVault {
    #if DEBUG
    private static let admissionLock = NSLock()
    private static var admission: NativeVaultAdmission?
    static func lastAdmission() -> NativeVaultAdmission? {
        admissionLock.lock(); defer { admissionLock.unlock() }
        return admission
    }
    static func diagnosticScopeAllowed(_ nonce: String) -> Bool { smokeAllowed(nonce) }
    static func diagnosticSelection(_ nonce: String) -> [String: Any]? {
        guard var selection = query(smokeNonce: nonce) else { return nil }
        selection[kSecAttrAccount as String] = "diagnostic-" + nonce
        return selection
    }
    #endif
    private static func smokeAllowed(_ nonce: String) -> Bool {
        #if DEBUG
        return nonce.count == 32 && nonce.allSatisfy({ "0123456789abcdef".contains($0) })
            && ProcessInfo.processInfo.environment["ZNET_IOS_UI_SMOKE_NONCE"] == nonce
        #else
        return false
        #endif
    }
    private static func query(smokeNonce: String?) -> [String: Any]? {
        guard let identity = Bundle.main.bundleIdentifier, !identity.isEmpty else {
            return nil
        }
        if let nonce = smokeNonce, !smokeAllowed(nonce) { return nil }
        return [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: identity + ".plugin-vault.v1",
            kSecAttrAccount as String: smokeNonce.map { "smoke-" + $0 } ?? "master",
            kSecAttrSynchronizable as String: false,
            kSecUseDataProtectionKeychain as String: true,
            kSecUseAuthenticationUI as String: kSecUseAuthenticationUIFail
        ]
    }
    static func probeStatus(_ query: [String: Any]) -> OSStatus {
        var selection = query
        // Check API authority without returning or creating key material.
        selection[kSecReturnData as String] = false
        selection[kSecReturnAttributes as String] = true
        selection[kSecMatchLimit as String] = kSecMatchLimitOne
        return SecItemCopyMatching(selection as CFDictionary, nil)
    }
    static func available(protectedDataAvailable: Bool) -> Bool {
        let selection = protectedDataAvailable ? query(smokeNonce: nil) : nil
        let status = selection.map { probeStatus($0) }
        let available = status == errSecSuccess || status == errSecItemNotFound
        #if DEBUG
        admissionLock.lock()
        admission = NativeVaultAdmission(protectedDataAvailable: protectedDataAvailable,
                                         status: status, apiAvailable: available)
        admissionLock.unlock()
        #endif
        return available
    }
    static func load(create: Bool, smokeNonce: String? = nil) -> [String: String] {
        guard let query = Self.query(smokeNonce: smokeNonce) else { return unavailable() }
        func read() -> (OSStatus, Data?) {
            var selection = query
            selection[kSecReturnData as String] = true
            selection[kSecMatchLimit as String] = kSecMatchLimitOne
            var value: CFTypeRef?
            let status = SecItemCopyMatching(selection as CFDictionary, &value)
            return (status, value as? Data)
        }
        func reply(_ status: OSStatus, _ data: Data?) -> [String: String] {
            guard status == errSecSuccess, var data = data, data.count == 32 else {
                return unavailable(status == errSecInteractionNotAllowed ? "protected-data-unavailable" : "secure-store-unavailable")
            }
            defer { data.resetBytes(in: 0..<data.count) }
            return ["status": "available", "keyBase64": data.base64EncodedString()]
        }
        let (status, data) = read()
        if status == errSecSuccess { return reply(status, data) }
        guard status == errSecItemNotFound else { return reply(status, nil) }
        guard create else { return ["status": "missing"] }
        var key = Data(count: 32)
        defer { key.resetBytes(in: 0..<key.count) }
        guard key.withUnsafeMutableBytes({ bytes in
            SecRandomCopyBytes(kSecRandomDefault, bytes.count, bytes.baseAddress!)
        }) == errSecSuccess else { return unavailable() }
        var insertion = query
        insertion[kSecUseAuthenticationUI as String] = nil
        insertion[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        insertion[kSecValueData as String] = key
        let added = SecItemAdd(insertion as CFDictionary, nil)
        guard added == errSecSuccess || added == errSecDuplicateItem else { return reply(added, nil) }
        // A concurrent creator's authoritative item is returned, never replaced.
        let current = read()
        return reply(current.0, current.1)
    }
    #if DEBUG
    static func clearSmoke(_ nonce: String) -> [String: String] {
        guard smokeAllowed(nonce), let selection = query(smokeNonce: nonce) else { return unavailable() }
        let status = SecItemDelete(selection as CFDictionary)
        return status == errSecSuccess || status == errSecItemNotFound ? ["status": "cleared"] : unavailable()
    }
    #endif
    private static func unavailable(_ reason: String = "secure-store-unavailable") -> [String: String] {
        ["status": "unavailable", "reason": reason]
    }
}
