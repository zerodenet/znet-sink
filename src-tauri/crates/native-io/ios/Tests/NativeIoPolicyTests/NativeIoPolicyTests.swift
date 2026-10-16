import Foundation
import XCTest
@testable import NativeIoPolicy

final class NativeIoPolicyTests: XCTestCase {
    private func request(purpose: String = "settings-export", limit: Int = 100,
                         name: String = "settings.json", mime: String = "application/json",
                         payload: String = "e30=", acknowledged: Bool? = nil,
                         id: String = "op_1-2") -> NativeIoRequest {
        NativeIoRequest(operationId: id, purpose: purpose, maximumBytes: limit,
                        fileName: name, mimeType: mime, payloadBase64: payload,
                        credentialsAcknowledged: acknowledged)
    }

    private func expect(_ expected: NativeIoError, _ operation: () throws -> Void,
                        file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertThrowsError(try operation(), file: file, line: line) {
            XCTAssertEqual($0 as? NativeIoError, expected, file: file, line: line)
        }
    }

    func testPurposeActionIntersectionAndProtectedMaterialRejection() throws {
        let config = NativeIoRequest(operationId: "read", purpose: "configuration-import",
                                     maximumBytes: 16 * 1024 * 1024)
        try config.validate(action: .pickRead)
        expect(.invalidPurpose) { try config.validate(action: .share) }
        expect(.invalidPurpose) { try request(purpose: "protected-material-export").validate(action: .save) }
        expect(.invalidPurpose) { try request(purpose: "settings-import").validate(action: .save) }
        expect(.invalidRequest) { try request(purpose: "settings-import").validate(action: .pickRead) }
    }

    func testCredentialAcknowledgmentIsExplicitForOrdinaryConfig() throws {
        expect(.credentialsNotAcknowledged) {
            try request(purpose: "ordinary-configuration-export").validate(action: .save)
        }
        expect(.credentialsNotAcknowledged) {
            try request(purpose: "ordinary-configuration-export", acknowledged: false).validate(action: .share)
        }
        XCTAssertEqual(try request(purpose: "ordinary-configuration-export", acknowledged: true)
            .exportData(action: .share), Data("{}".utf8))
    }

    func testOperationIdAndFilenameCannotBecomePaths() {
        for id in ["", "../bad", "a b", "a\n", "é", String(repeating: "a", count: 65)] {
            expect(.invalidOperationId) { try request(id: id).validate(action: .save) }
        }
        for name in ["", ".", "..", ".secret", "../secret", "a/b", "a\\b", "a:b", "x\n", "é.json",
                     String(repeating: "a", count: 129)] {
            expect(.invalidFileName) { try request(name: name).validate(action: .save) }
        }
    }

    func testMimeAndPurposeBudgets() throws {
        expect(.invalidMimeType) { try request(mime: "application/zip").validate(action: .save) }
        expect(.invalidMimeType) { try request(mime: "text/plain").validate(action: .save) }
        expect(.invalidMimeType) { try request(mime: "text/html").validate(action: .share) }
        try request(purpose: "redacted-diagnostic-export", mime: "application/zip").validate(action: .share)
        for limit in [0, -1, 2 * 1024 * 1024 + 1, Int.max] {
            expect(.invalidMaximumBytes) { try request(limit: limit).validate(action: .save) }
        }
        expect(.invalidMaximumBytes) {
            try request(purpose: "redacted-diagnostic-export", limit: 10 * 1024 * 1024 + 1)
                .validate(action: .save)
        }
    }

    func testEncodedAndDecodedBudgetAndStrictBase64() throws {
        expect(.sizeExceeded) { _ = try request(limit: 1, payload: "YWJjZA==").exportData(action: .save) }
        // Three decoded bytes fit in four encoded characters; encoded-length alone is insufficient.
        expect(.sizeExceeded) { _ = try request(limit: 1, payload: "YWJj").exportData(action: .save) }
        expect(.invalidPayload) { _ = try request(payload: "!!!!").exportData(action: .save) }
        expect(.invalidPayload) { _ = try request(payload: "e3\n0=").exportData(action: .save) }
        XCTAssertEqual(try request(limit: 2).exportData(action: .save).count, 2)
    }

    func testBoundedReadAtLimitAndAboveWithoutMetadata() throws {
        let bytes = Data(repeating: 7, count: 65537)
        XCTAssertEqual(try BoundedNativeIo.read(stream: InputStream(data: bytes), maximumBytes: bytes.count), bytes)
        expect(.sizeExceeded) {
            _ = try BoundedNativeIo.read(stream: InputStream(data: bytes), maximumBytes: bytes.count - 1)
        }
        XCTAssertEqual(try BoundedNativeIo.read(stream: InputStream(data: Data()), maximumBytes: 1), Data())
        expect(.readFailed) {
            _ = try BoundedNativeIo.read(url: URL(string: "https://example.invalid")!, maximumBytes: 1)
        }
        expect(.readFailed) {
            let missing = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
            _ = try BoundedNativeIo.read(url: missing, maximumBytes: 1)
        }
    }

    func testPrivateLeasesDoNotShareIdentityOrDeleteOtherData() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let sentinel = root.appendingPathComponent("keep.json")
        try Data("keep".utf8).write(to: sentinel)
        let first = try NativeFileLease(root: root, fileName: "data.json", data: Data("one".utf8))
        let second = try NativeFileLease(root: root, fileName: "data.json", data: Data("two".utf8))
        XCTAssertNotEqual(first.directory, second.directory)
        XCTAssertEqual(try BoundedNativeIo.read(url: first.file, maximumBytes: 3), Data("one".utf8))
        first.cleanup()
        first.cleanup()
        XCTAssertFalse(FileManager.default.fileExists(atPath: first.directory.path))
        XCTAssertEqual(try Data(contentsOf: second.file), Data("two".utf8))
        XCTAssertEqual(try Data(contentsOf: sentinel), Data("keep".utf8))
    }

    func testWireHasCamelCaseContentAndNeverAPath() throws {
        let response = NativeIoResponse(operationId: "id", status: "read", bytesRead: 2,
                                        contentBase64: "e30=")
        let data = try JSONEncoder().encode(response)
        let json = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
        XCTAssertEqual(Set(json.keys), Set(["operationId", "status", "bytesRead", "contentBase64"]))
        XCTAssertNil(json["path"])
        XCTAssertNil(json["url"])
    }
}
