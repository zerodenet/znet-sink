import Foundation
import XCTest
@testable import NativeIoPolicy

final class NativeVaultRequestTests: XCTestCase {
    func testOnlyAnExplicitBooleanMayCreateAKey() throws {
        let decoder = JSONDecoder()
        for text in ["{}", "{\"create\":1}", "{\"create\":\"true\"}", "{\"create\":true,\"path\":\"guest\"}", "[]"] {
            XCTAssertThrowsError(try decoder.decode(NativeVaultRequest.self, from: Data(text.utf8)))
        }
        XCTAssertTrue(try decoder.decode(NativeVaultRequest.self, from: Data("{\"create\":true}".utf8)).create)
        XCTAssertFalse(try decoder.decode(NativeVaultRequest.self, from: Data("{\"create\":false}".utf8)).create)
    }
    func testSmokeSelectorIsBoundedAndOnlyAvailableInDebugParsing() throws {
        let decoder=JSONDecoder()
        for nonce in ["short",String(repeating:"A",count:32),String(repeating:"a",count:33)] {
            let data=try JSONSerialization.data(withJSONObject:["create":true,"smokeNonce":nonce])
            XCTAssertThrowsError(try decoder.decode(NativeVaultRequest.self,from:data))
        }
        let data=try JSONSerialization.data(withJSONObject:["create":false,"smokeNonce":String(repeating:"a",count:32)])
        #if DEBUG
        XCTAssertEqual(try decoder.decode(NativeVaultRequest.self,from:data).smokeNonce,String(repeating:"a",count:32))
        #else
        XCTAssertThrowsError(try decoder.decode(NativeVaultRequest.self,from:data))
        #endif
    }
}
