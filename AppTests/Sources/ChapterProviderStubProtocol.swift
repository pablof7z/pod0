import Foundation

final class ChapterProviderStubProtocol: URLProtocol, @unchecked Sendable {
    nonisolated(unsafe) static var responseStatus = 200
    nonisolated(unsafe) static var responseHeaders: [String: String] = [
        "Content-Type": "application/json",
    ]
    nonisolated(unsafe) static var responseBody = Data()
    nonisolated(unsafe) static var error: Error?
    nonisolated(unsafe) static var lastRequest: URLRequest?
    nonisolated(unsafe) static var requestCount = 0

    static func reset() {
        responseStatus = 200
        responseHeaders = ["Content-Type": "application/json"]
        responseBody = Data()
        error = nil
        lastRequest = nil
        requestCount = 0
    }

    override class func canInit(with _: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        Self.requestCount += 1
        var captured = request
        if let stream = request.httpBodyStream {
            stream.open()
            defer { stream.close() }
            var body = Data()
            let bufferSize = 16 * 1_024
            let buffer = UnsafeMutablePointer<UInt8>.allocate(capacity: bufferSize)
            defer { buffer.deallocate() }
            while stream.hasBytesAvailable {
                let count = stream.read(buffer, maxLength: bufferSize)
                if count <= 0 { break }
                body.append(buffer, count: count)
            }
            captured.httpBody = body
        }
        Self.lastRequest = captured
        if let error = Self.error {
            client?.urlProtocol(self, didFailWithError: error)
            return
        }
        let response = HTTPURLResponse(
            url: request.url!,
            statusCode: Self.responseStatus,
            httpVersion: "HTTP/1.1",
            headerFields: Self.responseHeaders
        )!
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: Self.responseBody)
        client?.urlProtocolDidFinishLoading(self)
    }

    override func stopLoading() { }
}
