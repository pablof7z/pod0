import Foundation

extension LiveChapterModelTransport {
    static func httpFailure(
        _ status: UInt16,
        retryAfter: String?,
        now: Date
    ) -> ChapterCapabilityFailure {
        let code: ChapterCapabilityFailureCode = switch status {
        case 401, 403: .authentication
        case 413: .responseTooLarge
        default: .transport
        }
        return ChapterCapabilityFailure(
            code: code,
            httpStatus: status,
            safeDetail: "Chapter model HTTP \(status)",
            retryAfterMilliseconds: retryAfterMilliseconds(retryAfter, now: now)
        )
    }

    private static func retryAfterMilliseconds(
        _ rawValue: String?,
        now: Date
    ) -> UInt64? {
        guard let rawValue else { return nil }
        let value = rawValue.trimmingCharacters(in: .whitespacesAndNewlines)
        let maximum: UInt64 = 86_400_000
        if let seconds = UInt64(value) {
            let milliseconds = seconds.multipliedReportingOverflow(by: 1_000)
            return milliseconds.overflow ? maximum : min(milliseconds.partialValue, maximum)
        }
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = TimeZone(secondsFromGMT: 0)
        formatter.dateFormat = "EEE',' dd MMM yyyy HH':'mm':'ss z"
        guard let date = formatter.date(from: value) else { return nil }
        let interval = max(0, date.timeIntervalSince(now))
        return min(UInt64((interval * 1_000).rounded(.up)), maximum)
    }
}
