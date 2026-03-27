// Lit Node Simple API - C# SDK
//
// Wrapper for the v1 API endpoints defined in lit-node-simple-api.
// Mirrors the functionality of core_sdk.js.

using System.Net.Http.Json;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace LitNodeSimpleApi;

#region Response types

public class GetApiKeyResponse
{
    [JsonPropertyName("api_key")]
    public string ApiKey { get; set; } = string.Empty;
}

public class HandshakeResponse
{
    [JsonPropertyName("responses")]
    public JsonElement? Responses { get; set; }
}

public class MintPkpResponse
{
    [JsonPropertyName("pkp_public_key")]
    public string PkpPublicKey { get; set; } = string.Empty;
}

public class SignWithPkpResponse
{
    [JsonPropertyName("endpoint_responses")]
    public JsonElement? EndpointResponses { get; set; }
}

public class LitActionResponse
{
    [JsonPropertyName("execute_resp")]
    public JsonElement? ExecuteResp { get; set; }
}

#endregion

#region Request types

internal class SignWithPkpRequest
{
    [JsonPropertyName("api_key")]
    public string ApiKey { get; set; } = string.Empty;

    [JsonPropertyName("pkp_public_key")]
    public string PkpPublicKey { get; set; } = string.Empty;

    [JsonPropertyName("message")]
    public string Message { get; set; } = string.Empty;
}

internal class LitActionRequest
{
    [JsonPropertyName("api_key")]
    public string ApiKey { get; set; } = string.Empty;

    [JsonPropertyName("code")]
    public string Code { get; set; } = string.Empty;

    [JsonPropertyName("js_params")]
    public JsonElement? JsParams { get; set; }
}

#endregion

/// <summary>
/// Client for the Lit Node Simple API v1 endpoints.
/// </summary>
public class LitNodeSimpleApiClient
{
    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
        PropertyNameCaseInsensitive = true,
    };

    private readonly HttpClient _httpClient;
    private readonly string _baseUrl;

    /// <summary>
    /// Creates a client with the given base URL (trailing slash removed).
    /// </summary>
    /// <param name="baseUrl">Base URL of the API (default: http://localhost:8000).</param>
    /// <param name="httpClient">Optional HttpClient; if null, a new one is created.</param>
    public LitNodeSimpleApiClient(string baseUrl = "http://localhost:8000", HttpClient? httpClient = null)
    {
        _baseUrl = baseUrl.TrimEnd('/');
        _httpClient = httpClient ?? new HttpClient();
        _httpClient.BaseAddress = new Uri(_baseUrl + "/");
    }

    /// <summary>
    /// GET /get_api_key — Generates and returns a new API key (hex-encoded wallet secret).
    /// </summary>
    public async Task<GetApiKeyResponse> GetApiKeyAsync(CancellationToken cancellationToken = default)
    {
        var response = await _httpClient.GetAsync("get_api_key", cancellationToken).ConfigureAwait(false);
        response.EnsureSuccessStatusCode();
        var result = await response.Content.ReadFromJsonAsync<GetApiKeyResponse>(JsonOptions, cancellationToken).ConfigureAwait(false);
        return result ?? throw new InvalidOperationException("Empty response from get_api_key.");
    }

    /// <summary>
    /// GET /handshake — Performs handshake with validators and returns their responses.
    /// </summary>
    public async Task<HandshakeResponse> HandshakeAsync(CancellationToken cancellationToken = default)
    {
        var response = await _httpClient.GetAsync("handshake", cancellationToken).ConfigureAwait(false);
        response.EnsureSuccessStatusCode();
        var result = await response.Content.ReadFromJsonAsync<HandshakeResponse>(JsonOptions, cancellationToken).ConfigureAwait(false);
        return result ?? throw new InvalidOperationException("Empty response from handshake.");
    }

    /// <summary>
    /// GET /mint_pkp/{api_key} — Mints a new PKP for the wallet identified by the API key.
    /// </summary>
    /// <param name="apiKey">Hex-encoded API key (from GetApiKey).</param>
    public async Task<MintPkpResponse> MintPkpAsync(string apiKey, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrEmpty(apiKey))
            throw new ArgumentNullException(nameof(apiKey));
        var encoded = Uri.EscapeDataString(apiKey);
        var response = await _httpClient.GetAsync($"mint_pkp/{encoded}", cancellationToken).ConfigureAwait(false);
        response.EnsureSuccessStatusCode();
        var result = await response.Content.ReadFromJsonAsync<MintPkpResponse>(JsonOptions, cancellationToken).ConfigureAwait(false);
        return result ?? throw new InvalidOperationException("Empty response from mint_pkp.");
    }

    /// <summary>
    /// POST /sign_with_pkp — Signs a message with the given PKP using the provided API key.
    /// </summary>
    /// <param name="apiKey">Hex-encoded API key (from GetApiKey).</param>
    /// <param name="pkpPublicKey">PKP public key.</param>
    /// <param name="message">Message to sign.</param>
    public async Task<SignWithPkpResponse> SignWithPkpAsync(string apiKey, string pkpPublicKey, string message, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrEmpty(apiKey))
            throw new ArgumentNullException(nameof(apiKey));
        if (string.IsNullOrEmpty(pkpPublicKey))
            throw new ArgumentNullException(nameof(pkpPublicKey));
        var body = new SignWithPkpRequest
        {
            ApiKey = apiKey,
            PkpPublicKey = pkpPublicKey,
            Message = message ?? string.Empty,
        };
        var response = await _httpClient.PostAsJsonAsync("sign_with_pkp", body, JsonOptions, cancellationToken).ConfigureAwait(false);
        response.EnsureSuccessStatusCode();
        var result = await response.Content.ReadFromJsonAsync<SignWithPkpResponse>(JsonOptions, cancellationToken).ConfigureAwait(false);
        return result ?? throw new InvalidOperationException("Empty response from sign_with_pkp.");
    }

    /// <summary>
    /// POST /lit_action — Executes a lit action with the given code and optional params.
    /// </summary>
    /// <param name="apiKey">Hex-encoded API key (from GetApiKey).</param>
    /// <param name="code">Lit action JavaScript code.</param>
    /// <param name="jsParams">Optional JSON params passed to the lit action (null to omit).</param>
    public async Task<LitActionResponse> LitActionAsync(string apiKey, string code, JsonElement? jsParams = null, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrEmpty(apiKey))
            throw new ArgumentNullException(nameof(apiKey));
        if (string.IsNullOrEmpty(code))
            throw new ArgumentNullException(nameof(code));
        var body = new LitActionRequest
        {
            ApiKey = apiKey,
            Code = code,
            JsParams = jsParams,
        };
        var response = await _httpClient.PostAsJsonAsync("lit_action", body, JsonOptions, cancellationToken).ConfigureAwait(false);
        response.EnsureSuccessStatusCode();
        var result = await response.Content.ReadFromJsonAsync<LitActionResponse>(JsonOptions, cancellationToken).ConfigureAwait(false);
        return result ?? throw new InvalidOperationException("Empty response from lit_action.");
    }

    /// <summary>
    /// Overload: LitAction with js_params as object (serialized to JSON).
    /// </summary>
    public async Task<LitActionResponse> LitActionAsync(string apiKey, string code, object? jsParams, CancellationToken cancellationToken = default)
    {
        JsonElement? element = null;
        if (jsParams != null)
        {
            var json = JsonSerializer.Serialize(jsParams);
            element = JsonSerializer.Deserialize<JsonElement>(json);
        }
        return await LitActionAsync(apiKey, code, element, cancellationToken).ConfigureAwait(false);
    }
}

/// <summary>
/// Factory for a default client.
/// </summary>
public static class LitNodeSimpleApi
{
    /// <summary>
    /// Creates a LitNodeSimpleApiClient with the given base URL.
    /// </summary>
    /// <param name="baseUrl">Base URL of the API (default: http://localhost:8000).</param>
    /// <param name="httpClient">Optional HttpClient.</param>
    public static LitNodeSimpleApiClient CreateClient(string baseUrl = "http://localhost:8000", HttpClient? httpClient = null)
    {
        return new LitNodeSimpleApiClient(baseUrl, httpClient);
    }
}
