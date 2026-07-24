import Foundation

enum ChapterModelTransportFailureCode: Equatable, Sendable {
    case invalidRequest
    case transport
    case authentication
    case coreUnavailable
    case cancelled
    case responseTooLarge
    case invalidResponseMetadata
}

struct ChapterModelTransportFailure: Error, Equatable, Sendable {
    let code: ChapterModelTransportFailureCode
    let httpStatus: UInt16?
    let safeDetail: String?
    let retryAfterMilliseconds: UInt64?

    init(
        code: ChapterModelTransportFailureCode,
        httpStatus: UInt16?,
        safeDetail: String?,
        retryAfterMilliseconds: UInt64? = nil
    ) {
        self.code = code
        self.httpStatus = httpStatus
        self.safeDetail = safeDetail
        self.retryAfterMilliseconds = retryAfterMilliseconds
    }

    static let cancelled = Self(
        code: .cancelled,
        httpStatus: nil,
        safeDetail: nil
    )

    static func invalidRequest(_ detail: String) -> Self {
        Self(code: .invalidRequest, httpStatus: nil, safeDetail: detail)
    }

    static func responseTooLarge(_ detail: String) -> Self {
        Self(code: .responseTooLarge, httpStatus: nil, safeDetail: detail)
    }

    static func invalidMetadata(_ detail: String) -> Self {
        Self(code: .invalidResponseMetadata, httpStatus: nil, safeDetail: detail)
    }
}
