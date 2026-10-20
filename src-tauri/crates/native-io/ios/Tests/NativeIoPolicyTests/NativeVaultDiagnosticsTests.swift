import Foundation
import XCTest
@testable import NativeIoPolicy

final class NativeVaultDiagnosticsTests: XCTestCase {
    func testProtectedDataGateKeepsUnexecutedStepsWithoutInventedStatus() throws {
        let value = NativeVaultSmokeDiagnostics(protectedDataAvailable: false,
            admission: NativeVaultAdmission(protectedDataAvailable: false, status: nil, apiAvailable: false))
        let json = try XCTUnwrap(JSONSerialization.jsonObject(with: JSONEncoder().encode(value)) as? [String: Any])
        XCTAssertEqual(json["stopStage"] as? String, "protectedDataGate")
        XCTAssertEqual(json["roundTrip"] as? Bool, false)
        for key in ["availabilityProbe", "readBeforeCreate", "random", "create", "readAfterCreate", "cleanup"] {
            let step = try XCTUnwrap(json[key] as? [String: Any])
            XCTAssertEqual(step["attempted"] as? Bool, false)
            XCTAssertNil(step["osStatus"])
        }
    }
    func testDiagnosticStatusAndIsolatedResultCannotContainKeyOrChangeAdmission() throws {
        var value = NativeVaultSmokeDiagnostics(protectedDataAvailable: true,
            admission: NativeVaultAdmission(protectedDataAvailable: true, status: -50, apiAvailable: false))
        value.availabilityProbe = NativeVaultStep(-50)
        value.create = NativeVaultStep(-34018)
        value.stopStage = .create
        let json = try XCTUnwrap(JSONSerialization.jsonObject(with: JSONEncoder().encode(value)) as? [String: Any])
        XCTAssertEqual((json["create"] as? [String: Any])?["osStatus"] as? Int, -34018)
        XCTAssertEqual((json["lastHostAdmission"] as? [String: Any])?["apiAvailable"] as? Bool, false)
        for key in ["keyBase64", "account", "service", "message", "url", "nativeSecureStorageAccepted"] {
            XCTAssertNil(json[key])
        }
    }
}
