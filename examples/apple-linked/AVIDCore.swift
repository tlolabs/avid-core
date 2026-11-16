// SPDX-License-Identifier: GPL-3.0-or-later
import Foundation
import AVIDCore

/// Keep an instance alive until its synchronous calls return; run on a worker.
public final class LinkedCore: @unchecked Sendable {
    private let core: UInt64
    public init() throws {
        guard avid_core_abi_version() == 1 else { throw Failure.abi }
        core = avid_core_create()
        guard core != 0 else { throw Failure.abi }
    }
    deinit { _ = avid_core_release(core) }
    public enum Failure: Error { case abi, response }
    public final class Cancellation: @unchecked Sendable {
        fileprivate let handle = avid_core_cancel_create()
        public init() {}
        public func cancel() { _ = avid_core_cancel(handle) }
        deinit { _ = avid_core_release(handle) }
    }
    /// Core returns schema JSON, including structured failures. No policy in Swift.
    public func execute(_ request: Data, cancellation: Cancellation,
                        event: @escaping (Data) -> Void = { _ in }) throws -> Data {
        let box = EventBox(event)
        let context = Unmanaged.passUnretained(box).toOpaque()
        let result = request.withUnsafeBytes { bytes in
            avid_core_execute_json(core, cancellation.handle,
                bytes.bindMemory(to: UInt8.self).baseAddress, bytes.count,
                { context, bytes, count in
                    guard let context, let bytes else { return }
                    Unmanaged<EventBox>.fromOpaque(context).takeUnretainedValue().call(Data(bytes: bytes, count: count))
                }, context)
        }
        defer { _ = avid_core_release(result) }
        let size = avid_core_buffer_len(result)
        guard size > 0 else { throw Failure.response }
        var output = Data(count: size)
        let copied = output.withUnsafeMutableBytes { avid_core_buffer_copy(result, $0.bindMemory(to: UInt8.self).baseAddress, $0.count) }
        guard copied == size else { throw Failure.response }
        withExtendedLifetime(box) {}
        return output
    }
}
private final class EventBox {
    let call: (Data) -> Void
    init(_ call: @escaping (Data) -> Void) { self.call = call }
}
