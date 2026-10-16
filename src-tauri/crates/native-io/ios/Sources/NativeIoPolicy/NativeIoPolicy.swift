import Foundation

public enum NativeIoAction { case pickRead, save, share }

public enum NativeIoError: String, Error {
    case invalidRequest = "invalid-request"
    case invalidOperationId = "invalid-operation-id"
    case invalidPurpose = "invalid-purpose"
    case invalidMaximumBytes = "invalid-maximum-bytes"
    case invalidFileName = "invalid-file-name"
    case invalidMimeType = "invalid-mime-type"
    case credentialsNotAcknowledged = "credentials-not-acknowledged"
    case invalidPayload = "invalid-payload"
    case sizeExceeded = "size-exceeded"
    case readFailed = "read-failed"
    case stageFailed = "stage-failed"
}

public struct NativeIoRequest: Decodable {
    public let operationId: String
    public let purpose: String
    public let maximumBytes: Int
    public let fileName: String?
    public let mimeType: String?
    public let payloadBase64: String?
    public let credentialsAcknowledged: Bool?

    public init(operationId: String, purpose: String, maximumBytes: Int,
                fileName: String? = nil, mimeType: String? = nil,
                payloadBase64: String? = nil, credentialsAcknowledged: Bool? = nil) {
        self.operationId = operationId
        self.purpose = purpose
        self.maximumBytes = maximumBytes
        self.fileName = fileName
        self.mimeType = mimeType
        self.payloadBase64 = payloadBase64
        self.credentialsAcknowledged = credentialsAcknowledged
    }

    public func validate(action: NativeIoAction) throws {
        guard !operationId.isEmpty, operationId.utf8.count <= 64,
              operationId.utf8.allSatisfy({ byte in
                  (65...90).contains(byte) || (97...122).contains(byte)
                    || (48...57).contains(byte) || byte == 45 || byte == 95
              }) else { throw NativeIoError.invalidOperationId }
        let budget: Int
        switch (action, purpose) {
        case (.pickRead, "configuration-import"):
            budget = 16 * 1024 * 1024
        case (.pickRead, "settings-import"):
            budget = 2 * 1024 * 1024
        case (.save, "ordinary-configuration-export"), (.share, "ordinary-configuration-export"):
            budget = 16 * 1024 * 1024
            guard credentialsAcknowledged == true else {
                throw NativeIoError.credentialsNotAcknowledged
            }
        case (.save, "settings-export"), (.share, "settings-export"):
            budget = 2 * 1024 * 1024
        case (.save, "redacted-diagnostic-export"), (.share, "redacted-diagnostic-export"):
            budget = 10 * 1024 * 1024
        default: throw NativeIoError.invalidPurpose
        }
        guard maximumBytes > 0, maximumBytes <= budget else {
            throw NativeIoError.invalidMaximumBytes
        }
        if action == .pickRead {
            guard fileName == nil, mimeType == nil, payloadBase64 == nil,
                  credentialsAcknowledged == nil else { throw NativeIoError.invalidRequest }
            return
        }
        guard let name = fileName, !name.isEmpty, name.utf8.count <= 128,
              name != ".", name != "..", !name.hasPrefix("."),
              name.utf8.allSatisfy({ byte in
                  (65...90).contains(byte) || (97...122).contains(byte)
                    || (48...57).contains(byte) || byte == 45 || byte == 95 || byte == 46
              }) else { throw NativeIoError.invalidFileName }
        guard let mime = mimeType,
              ["application/json", "text/plain", "application/zip"].contains(mime),
              (purpose == "redacted-diagnostic-export" || mime == "application/json") else {
            throw NativeIoError.invalidMimeType
        }
        // Reject oversized encoded input before allocating a decoded buffer. Decode is strict.
        guard let encoded = payloadBase64,
              encoded.utf8.count <= ((maximumBytes + 2) / 3) * 4 else {
            throw NativeIoError.sizeExceeded
        }
    }

    public func exportData(action: NativeIoAction) throws -> Data {
        try validate(action: action)
        guard action != .pickRead, let encoded = payloadBase64,
              let data = Data(base64Encoded: encoded) else { throw NativeIoError.invalidPayload }
        guard data.count <= maximumBytes else { throw NativeIoError.sizeExceeded }
        return data
    }
}

public struct NativeIoResponse: Encodable {
    public let operationId: String
    public let status: String
    public let bytesRead: Int?
    public let contentBase64: String?
    public let reason: String?

    public init(operationId: String, status: String, bytesRead: Int? = nil,
                contentBase64: String? = nil, reason: String? = nil) {
        self.operationId = operationId
        self.status = status
        self.bytesRead = bytesRead
        self.contentBase64 = contentBase64
        self.reason = reason
    }
}

public enum BoundedNativeIo {
    /// Reads at most limit+1 bytes, so metadata races or a growing provider cannot bypass the budget.
    public static func read(stream: InputStream, maximumBytes: Int) throws -> Data {
        guard maximumBytes > 0, maximumBytes <= 16 * 1024 * 1024 else {
            throw NativeIoError.invalidMaximumBytes
        }
        stream.open()
        defer { stream.close() }
        var data = Data()
        var chunk = [UInt8](repeating: 0, count: 32 * 1024)
        while true {
            let count = stream.read(&chunk, maxLength: min(chunk.count, maximumBytes - data.count + 1))
            guard count >= 0 else { throw NativeIoError.readFailed }
            if count == 0 { return data }
            guard count <= maximumBytes - data.count else { throw NativeIoError.sizeExceeded }
            data.append(contentsOf: chunk[0..<count])
        }
    }

    public static func read(url: URL, maximumBytes: Int) throws -> Data {
        guard url.isFileURL, let stream = InputStream(url: url) else {
            throw NativeIoError.readFailed
        }
        return try read(stream: stream, maximumBytes: maximumBytes)
    }
}

/// Each export owns a separate private directory. No consumer-supplied directory is used.
public final class NativeFileLease {
    public let directory: URL
    public let file: URL
    private let fileManager: FileManager

    public init(root: URL, fileName: String, data: Data,
                fileManager: FileManager = .default) throws {
        // Reuse safe-leaf validation even for internal callers.
        let check = NativeIoRequest(operationId: "lease", purpose: "settings-export",
                                    maximumBytes: 1, fileName: fileName,
                                    mimeType: "application/json", payloadBase64: "")
        try check.validate(action: .save)
        self.fileManager = fileManager
        directory = root.appendingPathComponent(UUID().uuidString, isDirectory: true)
        file = directory.appendingPathComponent(fileName, isDirectory: false)
        do {
            try fileManager.createDirectory(at: directory, withIntermediateDirectories: true,
                                            attributes: [.posixPermissions: 0o700])
            try data.write(to: file, options: [.atomic])
            try fileManager.setAttributes([.posixPermissions: 0o600], ofItemAtPath: file.path)
        } catch {
            try? fileManager.removeItem(at: directory)
            throw NativeIoError.stageFailed
        }
    }

    public func cleanup() { try? fileManager.removeItem(at: directory) }
    deinit { cleanup() }
}
