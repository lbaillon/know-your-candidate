from httpx import AsyncClient

from kyc_api.http_cache import CACHE_CONTROL, compute_etag


def test_compute_etag_is_stable_for_the_same_body() -> None:
    assert compute_etag(b"hello") == compute_etag(b"hello")


def test_compute_etag_differs_for_a_different_body() -> None:
    assert compute_etag(b"hello") != compute_etag(b"world")


async def test_a_public_page_carries_an_etag_and_cache_control(client: AsyncClient) -> None:
    response = await client.get("/methodologie")

    assert response.status_code == 200
    assert response.headers["cache-control"] == CACHE_CONTROL
    assert response.headers["etag"].startswith('"')


async def test_a_matching_if_none_match_yields_304(client: AsyncClient) -> None:
    first = await client.get("/methodologie")
    etag = first.headers["etag"]

    second = await client.get("/methodologie", headers={"if-none-match": etag})

    assert second.status_code == 304
    assert second.headers["etag"] == etag


async def test_a_stale_if_none_match_yields_200(client: AsyncClient) -> None:
    response = await client.get("/methodologie", headers={"if-none-match": '"stale"'})

    assert response.status_code == 200


async def test_healthz_is_never_cached(client: AsyncClient) -> None:
    response = await client.get("/healthz")

    assert "etag" not in response.headers
    assert "cache-control" not in response.headers


async def test_a_404_carries_no_etag(client: AsyncClient) -> None:
    response = await client.get("/cette-route-n-existe-pas")

    assert response.status_code == 404
    assert "etag" not in response.headers
