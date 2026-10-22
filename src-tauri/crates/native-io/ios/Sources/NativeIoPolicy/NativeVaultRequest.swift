import Foundation

/// Fixed host-only request. Numbers/strings and unknown keys cannot create material.
public struct NativeVaultRequest: Decodable {
    public let create: Bool
    public let smokeNonce: String?
    public init(create: Bool, smokeNonce: String? = nil) { self.create = create; self.smokeNonce = smokeNonce }
    private struct Field: CodingKey {
        let stringValue: String
        var intValue: Int? { nil }
        init(_ value: String) { stringValue = value }
        init?(stringValue: String) { self.stringValue = stringValue }
        init?(intValue: Int) { return nil }
    }
    public init(from decoder: Decoder) throws {
        let fields = try decoder.container(keyedBy: Field.self)
        var allowed: Set<String> = ["create"]
        #if DEBUG
        allowed.insert("smokeNonce")
        #endif
        guard Set(fields.allKeys.map(\.stringValue)).isSubset(of: allowed) else {
            throw NativeIoError.invalidRequest
        }
        create = try fields.decode(Bool.self, forKey: Field("create"))
        #if DEBUG
        smokeNonce = try fields.decodeIfPresent(String.self, forKey: Field("smokeNonce"))
        if let nonce = smokeNonce {
            guard nonce.count == 32, nonce.allSatisfy({ "0123456789abcdef".contains($0) }) else {
                throw NativeIoError.invalidRequest
            }
        }
        #else
        smokeNonce = nil
        #endif
    }
}
