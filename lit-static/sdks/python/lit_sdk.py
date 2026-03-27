"""
Lit Node Simple API - Python SDK

Wrapper for the v1 API endpoints defined in lit-node-simple-api.
Mirrors the functionality of core_sdk.js.
"""

from __future__ import annotations

import json
import urllib.error
import urllib.parse
import urllib.request
from typing import Any


DEFAULT_BASE_URL = "http://localhost:8000"


class LitNodeSimpleApiClient:
    """
    Client for the Lit Node Simple API v1 endpoints.
    """

    def __init__(self, base_url: str = DEFAULT_BASE_URL) -> None:
        """
        Create a client with the given base URL (trailing slash removed).

        :param base_url: Base URL of the API (default: http://localhost:8000).
        """
        self._base_url = base_url.rstrip("/")

    def _request(
        self,
        method: str,
        path: str,
        body: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        url = f"{self._base_url}{path}"
        data = None
        if body is not None:
            data = json.dumps(body).encode("utf-8")
        req = urllib.request.Request(
            url,
            data=data,
            method=method,
            headers={"Content-Type": "application/json"} if data else {},
        )
        try:
            with urllib.request.urlopen(req) as resp:
                return json.loads(resp.read().decode("utf-8"))
        except urllib.error.HTTPError as e:
            msg = e.read().decode("utf-8") if e.fp else e.reason or str(e)
            raise RuntimeError(f"{e.code} {e.reason}: {msg}") from e

    def get_api_key(self) -> dict[str, Any]:
        """
        GET /get_api_key — Generate and return a new API key (hex-encoded wallet secret).

        :return: Dict with key "api_key".
        """
        return self._request("GET", "/get_api_key")

    def handshake(self) -> dict[str, Any]:
        """
        GET /handshake — Perform handshake with validators and return their responses.

        :return: Dict with key "responses".
        """
        return self._request("GET", "/handshake")

    def mint_pkp(self, api_key: str) -> dict[str, Any]:
        """
        GET /mint_pkp/<api_key> — Mint a new PKP for the wallet identified by the API key.

        :param api_key: Hex-encoded API key (from get_api_key).
        :return: Dict with key "pkp_public_key".
        """
        if not api_key:
            raise ValueError("api_key is required")
        path = "/mint_pkp/" + urllib.parse.quote(api_key, safe="")
        return self._request("GET", path)

    def sign_with_pkp(
        self,
        *,
        api_key: str,
        pkp_public_key: str,
        message: str,
    ) -> dict[str, Any]:
        """
        POST /sign_with_pkp — Sign a message with the given PKP using the provided API key.

        :param api_key: Hex-encoded API key (from get_api_key).
        :param pkp_public_key: PKP public key.
        :param message: Message to sign.
        :return: Dict with key "endpoint_responses".
        """
        if not api_key:
            raise ValueError("api_key is required")
        if not pkp_public_key:
            raise ValueError("pkp_public_key is required")
        return self._request(
            "POST",
            "/sign_with_pkp",
            body={
                "api_key": api_key,
                "pkp_public_key": pkp_public_key,
                "message": message,
            },
        )

    def lit_action(
        self,
        *,
        api_key: str,
        code: str,
        js_params: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        """
        POST /lit_action — Execute a lit action with the given code and optional params.

        :param api_key: Hex-encoded API key (from get_api_key).
        :param code: Lit action JavaScript code.
        :param js_params: Optional JSON-serializable params passed to the lit action.
        :return: Dict with key "execute_resp".
        """
        if not api_key:
            raise ValueError("api_key is required")
        if not code:
            raise ValueError("code is required")
        body: dict[str, Any] = {
            "api_key": api_key,
            "code": code,
            "js_params": js_params,
        }
        return self._request("POST", "/lit_action", body=body)


def create_client(base_url: str = DEFAULT_BASE_URL) -> LitNodeSimpleApiClient:
    """
    Create a LitNodeSimpleApiClient with the given base URL.

    :param base_url: Base URL of the API (default: http://localhost:8000).
    :return: LitNodeSimpleApiClient instance.
    """
    return LitNodeSimpleApiClient(base_url=base_url)
