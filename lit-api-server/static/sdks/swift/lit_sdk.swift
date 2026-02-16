/**
 Lit Node Simple API - Swift SDK

 Wrapper for the v1 API endpoints defined in lit-node-simple-api.
 Mirrors the functionality of core_sdk.js.

 Uses: Foundation URLSession, async/await (Swift 5.5+).
 */

import Foundation

public let litNodeSimpleAPIDefaultBaseURL = "http://localhost:8000"

/// Client for the Lit Node Simple API v1 endpoints.
public final class LitNodeSimpleApiClient: Sendable {

    private let baseURL: String
    private let session: URLSession

    /// Creates a client with the given base URL (trailing slash removed).
    /// - Parameters:
    ///   - baseURL: Base URL of the API (default: http://localhost:8000).
    ///   - session: Optional URLSession; uses shared by default.
    public init(
        baseURL: String = litNodeSimpleAPIDefaultBaseURL,
        session: URLSession = .shared
    ) {
        self.baseURL = baseURL.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        self.session = session
    }

    /// GET /get_api_key — Generates and returns a new API key (hex-encoded wallet secret).
    /// - Returns: Response dict with key "api_key".
    public func getApiKey() async throws -> [String: Any] {
        try await request("GET", path: "/get_api_key", body: nil)
    }

    /// GET /handshake — Performs handshake with validators and returns their responses.
    /// - Returns: Response dict with key "responses".
    public func handshake() async throws -> [String: Any] {
        try await request("GET", path: "/handshake", body: nil)
    }

    /// GET /mint_pkp/{api_key} — Mints a new PKP for the wallet identified by the API key.
    /// - Parameter apiKey: Hex-encoded API key (from getApiKey).
    /// - Returns: Response dict with key "pkp_public_key".
    public func mintPkp(apiKey: String) async throws -> [String: Any] {
        guard !apiKey.isEmpty else { throw LitNodeSimpleAPIError.invalidParameter("api_key is required") }
        let encoded = apiKey.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? apiKey
        return try await request("GET", path: "/mint_pkp/\(encoded)", body: nil)
    }

    /// POST /sign_with_pkp — Signs a message with the given PKP using the provided API key.
    /// - Returns: Response dict with key "endpoint_responses".
    public func signWithPkp(apiKey: String, pkpPublicKey: String, message: String) async throws -> [String: Any] {
        guard !apiKey.isEmpty else { throw LitNodeSimpleAPIError.invalidParameter("api_key is required") }
        guard !pkpPublicKey.isEmpty else { throw LitNodeSimpleAPIError.invalidParameter("pkp_public_key is required") }
        let body: [String: Any] = [
            "api_key": apiKey,
            "pkp_public_key": pkpPublicKey,
            "message": message
        ]
        return try await request("POST", path: "/sign_with_pkp", body: body)
    }

    /// POST /lit_action — Executes a lit action with the given code and optional params.
    /// - Parameters:
    ///   - apiKey: Hex-encoded API key (from getApiKey).
    ///   - code: Lit action JavaScript code.
    ///   - jsParams: Optional JSON-serializable params (nil to omit).
    /// - Returns: Response dict with key "execute_resp".
    public func litAction(apiKey: String, code: String, jsParams: [String: Any]? = nil) async throws -> [String: Any] {
        guard !apiKey.isEmpty else { throw LitNodeSimpleAPIError.invalidParameter("api_key is required") }
        guard !code.isEmpty else { throw LitNodeSimpleAPIError.invalidParameter("code is required") }
        let body: [String: Any] = [
            "api_key": apiKey,
            "code": code,
            "js_params": jsParams ?? NSNull()
        ]
        return try await request("POST", path: "/lit_action", body: body)
    }

    private func request(_ method: String, path: String, body: [String: Any]?) async throws -> [String: Any] {
        let urlString = "\(baseURL)\(path)"
        guard let url = URL(string: urlString) else {
            throw LitNodeSimpleAPIError.invalidURL(urlString)
        }
        var request = URLRequest(url: url)
        request.httpMethod = method
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        if let body = body {
            request.httpBody = try JSONSerialization.data(withJSONObject: body)
        }
        let (data, response) = try await session.data(for: request)
        guard let http = response as? HTTPURLResponse else {
            throw LitNodeSimpleAPIError.invalidResponse
        }
        if http.statusCode < 200 || http.statusCode >= 300 {
            let message = String(data: data, encoding: .utf8) ?? ""
            throw LitNodeSimpleAPIError.httpError(statusCode: http.statusCode, body: message)
        }
        guard let json = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw LitNodeSimpleAPIError.invalidJSON
        }
        return json
    }
}

/// Errors thrown by the SDK.
public enum LitNodeSimpleAPIError: Error, Sendable {
    case invalidParameter(String)
    case invalidURL(String)
    case invalidResponse
    case invalidJSON
    case httpError(statusCode: Int, body: String)
}

/// Factory for a default client.
/// - Parameter baseURL: Base URL of the API (default: http://localhost:8000).
/// - Returns: A new LitNodeSimpleApiClient.
public func createClient(baseURL: String = litNodeSimpleAPIDefaultBaseURL) -> LitNodeSimpleApiClient {
    LitNodeSimpleApiClient(baseURL: baseURL)
}
