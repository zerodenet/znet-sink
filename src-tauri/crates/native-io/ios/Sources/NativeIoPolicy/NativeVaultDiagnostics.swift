import Foundation

/// Debug evidence contains fixed phases and OSStatus values only, never key material.
public struct NativeVaultStep: Encodable {
    public let attempted: Bool
    public let osStatus: Int32?
    public init(_ status: Int32? = nil) { attempted = status != nil; osStatus = status }
}
public struct NativeVaultAdmission: Encodable {
    public let protectedDataAvailable: Bool
    public let probe: NativeVaultStep
    public let apiAvailable: Bool
    public init(protectedDataAvailable: Bool, status: Int32?, apiAvailable: Bool) {
        self.protectedDataAvailable = protectedDataAvailable
        probe = NativeVaultStep(status)
        self.apiAvailable = apiAvailable
    }
}
public enum NativeVaultStopStage: String, Encodable, Equatable {
    case scope, protectedDataGate, readBeforeCreate, random, create, readAfterCreate, cleanup, completed
}
public struct NativeVaultSmokeDiagnostics: Encodable {
    public let schemaVersion = 1
    public let isolated = true
    public let protectedDataAvailable: Bool
    public let lastHostAdmission: NativeVaultAdmission?
    public var availabilityProbe = NativeVaultStep()
    public var readBeforeCreate = NativeVaultStep()
    public var random = NativeVaultStep()
    public var create = NativeVaultStep()
    public var readAfterCreate = NativeVaultStep()
    public var cleanup = NativeVaultStep()
    public var readBackMatches: Bool?
    public var stopStage: NativeVaultStopStage
    public var roundTrip = false
    public init(protectedDataAvailable: Bool, admission: NativeVaultAdmission?) {
        self.protectedDataAvailable = protectedDataAvailable
        lastHostAdmission = admission
        stopStage = protectedDataAvailable ? .scope : .protectedDataGate
    }
}
