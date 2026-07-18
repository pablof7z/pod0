import Foundation
import Pod0Core
import XCTest

extension Pod0ListeningDomainBindingTests {
    func loadListeningFixture() throws -> [String: String] {
        let url = try XCTUnwrap(Bundle(for: Self.self).url(
            forResource: "listening-domain-v1",
            withExtension: "properties"
        ))
        return try String(contentsOf: url, encoding: .utf8)
            .split(whereSeparator: \.isNewline)
            .filter { !$0.isEmpty && !$0.hasPrefix("#") }
            .reduce(into: [:]) { output, line in
                let parts = line.split(
                    separator: "=",
                    maxSplits: 1,
                    omittingEmptySubsequences: false
                )
                if parts.count == 2 { output[String(parts[0])] = String(parts[1]) }
            }
    }

    func codableRoundTrip<T: Codable>(_ value: T) throws -> T {
        try JSONDecoder().decode(T.self, from: JSONEncoder().encode(value))
    }

    func legacyRoundTrip<T: Codable>(
        _ value: T,
        replacing keys: (String, String)?,
        removing: [String]
    ) throws -> T {
        var object = try XCTUnwrap(
            JSONSerialization.jsonObject(with: JSONEncoder().encode(value)) as? [String: Any]
        )
        if let keys { object[keys.1] = object.removeValue(forKey: keys.0) }
        removing.forEach { object.removeValue(forKey: $0) }
        object["futureFieldUnknownToV1"] = "ignored"
        return try JSONDecoder().decode(
            T.self,
            from: JSONSerialization.data(withJSONObject: object)
        )
    }

    func corePodcastID(_ id: UUID) -> PodcastId {
        let parts = uuidParts(id)
        return PodcastId(high: parts.high, low: parts.low)
    }

    func coreEpisodeID(_ id: UUID) -> EpisodeId {
        let parts = uuidParts(id)
        return EpisodeId(high: parts.high, low: parts.low)
    }

    func uuidParts(_ id: UUID) -> (high: UInt64, low: UInt64) {
        let hex = id.uuidString.replacingOccurrences(of: "-", with: "")
        return (
            UInt64(hex.prefix(16), radix: 16)!,
            UInt64(hex.suffix(16), radix: 16)!
        )
    }

    func uint(_ values: [String: String], _ key: String) -> UInt64 { UInt64(values[key]!)! }
    func int(_ values: [String: String], _ key: String) -> Int { Int(values[key]!)! }
    func date(_ values: [String: String], _ key: String) -> Date {
        Date(timeIntervalSince1970: Double(values[key]!)! / 1_000)
    }
    func seconds(_ values: [String: String], _ key: String) -> Double {
        Double(values[key]!)! / 1_000
    }
    func doublePermille(_ values: [String: String], _ key: String) -> Double {
        Double(values[key]!)! / 1_000
    }
    func epochMilliseconds(_ date: Date) -> Int64 {
        Int64((date.timeIntervalSince1970 * 1_000).rounded())
    }
    func durationMilliseconds(_ seconds: Double) -> Int64 {
        Int64((seconds * 1_000).rounded())
    }
}
