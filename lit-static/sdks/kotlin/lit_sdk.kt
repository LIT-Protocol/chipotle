/**
 * Lit Node Simple API - Kotlin SDK
 *
 * Wrapper for the v1 API endpoints defined in lit-node-simple-api.
 * Mirrors the functionality of core_sdk.js.
 *
 * Uses: Java 11+ [java.net.http.HttpClient], [kotlinx.serialization] for JSON.
 * Add to your build (e.g. build.gradle.kts):
 *   implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.6.0")
 *   implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.7.x")
 */

package lit.node.simple.api

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import java.net.URI
import java.net.URLEncoder
import java.net.http.HttpClient
import java.net.http.HttpRequest
import java.net.http.HttpResponse
import java.nio.charset.StandardCharsets
import java.time.Duration

private const val DEFAULT_BASE_URL = "http://localhost:8000"

private val json = Json {
    ignoreUnknownKeys = true
    encodeDefaults = true
}

// --- Response types (mirror JS res.json() shape) ---

@Serializable
data class GetApiKeyResponse(val api_key: String)

@Serializable
data class HandshakeResponse(val responses: JsonElement? = null)

@Serializable
data class MintPkpResponse(val pkp_public_key: String)

@Serializable
data class SignWithPkpResponse(val endpoint_responses: JsonElement? = null)

@Serializable
data class LitActionResponse(val execute_resp: JsonElement? = null)

/**
 * Client for the Lit Node Simple API v1 endpoints.
 */
class LitNodeSimpleApiClient(
    baseUrl: String = DEFAULT_BASE_URL,
    private val httpClient: HttpClient = HttpClient.newBuilder()
        .connectTimeout(Duration.ofSeconds(10))
        .build()
) {
    private val baseUrl: String = baseUrl.trimEnd('/')

    /**
     * GET /get_api_key — Generates and returns a new API key (hex-encoded wallet secret).
     */
    suspend fun getApiKey(): GetApiKeyResponse = request("GET", "/get_api_key", null, GetApiKeyResponse.serializer())

    /**
     * GET /handshake — Performs handshake with validators and returns their responses.
     */
    suspend fun handshake(): HandshakeResponse = request("GET", "/handshake", null, HandshakeResponse.serializer())

    /**
     * GET /mint_pkp/{api_key} — Mints a new PKP for the wallet identified by the API key.
     * @param apiKey Hex-encoded API key (from getApiKey).
     */
    suspend fun mintPkp(apiKey: String): MintPkpResponse {
        require(apiKey.isNotBlank()) { "api_key is required" }
        val path = "/mint_pkp/${URLEncoder.encode(apiKey, StandardCharsets.UTF_8)}"
        return request("GET", path, null, MintPkpResponse.serializer())
    }

    /**
     * POST /sign_with_pkp — Signs a message with the given PKP using the provided API key.
     */
    suspend fun signWithPkp(
        apiKey: String,
        pkpPublicKey: String,
        message: String
    ): SignWithPkpResponse {
        require(apiKey.isNotBlank()) { "api_key is required" }
        require(pkpPublicKey.isNotBlank()) { "pkp_public_key is required" }
        val body = buildJsonObject {
            put("api_key", apiKey)
            put("pkp_public_key", pkpPublicKey)
            put("message", message)
        }
        return request("POST", "/sign_with_pkp", body, SignWithPkpResponse.serializer())
    }

    /**
     * POST /lit_action — Executes a lit action with the given code and optional params.
     * @param apiKey Hex-encoded API key (from getApiKey).
     * @param code Lit action JavaScript code.
     * @param jsParams Optional JSON params passed to the lit action (null to omit).
     */
    suspend fun litAction(
        apiKey: String,
        code: String,
        jsParams: JsonElement? = null
    ): LitActionResponse {
        require(apiKey.isNotBlank()) { "api_key is required" }
        require(code.isNotBlank()) { "code is required" }
        val body = buildJsonObject {
            put("api_key", apiKey)
            put("code", code)
            if (jsParams != null) put("js_params", jsParams) else put("js_params", kotlinx.serialization.json.JsonNull)
        }
        return request("POST", "/lit_action", body, LitActionResponse.serializer())
    }

    private suspend fun <T> request(
        method: String,
        path: String,
        body: JsonObject?,
        serializer: kotlinx.serialization.KSerializer<T>
    ): T {
        val uri = URI.create("$baseUrl$path")
        val bodyPublisher = body?.let { HttpRequest.BodyPublishers.ofString(json.encodeToString(JsonObject.serializer(), it)) }
            ?: HttpRequest.BodyPublishers.noBody()
        val requestBuilder = HttpRequest.newBuilder(uri)
            .method(method, bodyPublisher)
            .timeout(Duration.ofSeconds(30))
        if (body != null) {
            requestBuilder.header("Content-Type", "application/json")
        }
        return withContext(Dispatchers.IO) {
            val response = httpClient.send(requestBuilder.build(), HttpResponse.BodyHandlers.ofString(StandardCharsets.UTF_8))
            if (response.statusCode() < 200 || response.statusCode() >= 300) {
                throw RuntimeException("${response.uri()} failed: ${response.statusCode()} ${response.body()}")
            }
            json.decodeFromString(serializer, response.body())
        }
    }
}

/**
 * Factory for a default client.
 */
fun createClient(baseUrl: String = DEFAULT_BASE_URL, httpClient: HttpClient? = null): LitNodeSimpleApiClient {
    return if (httpClient != null) {
        LitNodeSimpleApiClient(baseUrl = baseUrl, httpClient = httpClient)
    } else {
        LitNodeSimpleApiClient(baseUrl = baseUrl)
    }
</think>
Fixing the Kotlin file: removing the invalid Map overload and correcting the implementation.
<｜tool▁calls▁begin｜><｜tool▁call▁begin｜>
StrReplace