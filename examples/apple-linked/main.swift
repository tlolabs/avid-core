// Development host. Not signed/notarized. SPDX-License-Identifier: GPL-3.0-or-later
import Foundation
let core = try LinkedCore()
let cancel = LinkedCore.Cancellation()
let arguments=Array(CommandLine.arguments.dropFirst())
let shouldCancel=arguments.first == "--cancel-on-progress"
let file=arguments.last
let input = file.map { try! Data(contentsOf: URL(fileURLWithPath: $0)) }
    ?? Data(#"{"schema_version":1,"operation":"capabilities"}"#.utf8)
let output = try core.execute(input, cancellation: cancel) { data in
    FileHandle.standardError.write(data + Data([10]))
    if shouldCancel, let event=try? JSONSerialization.jsonObject(with:data) as? [String:Any], event["event"] as? String == "progress" { cancel.cancel() }
}
FileHandle.standardOutput.write(output + Data([10]))
let result=try JSONSerialization.jsonObject(with:output) as? [String:Any]
if result?["ok"] as? Bool != true { exit(10) }
