"""Tests de la garde CSRF (D3.19) — voir docs/plans/phase-3-categorisation.md, section
« Authentification admin ». `/admin/logout` sert de cible : une méthode non sûre sous `/admin`
qui n'exige pas d'être connecté, donc un test par branche sans avoir à simuler GitHub.
"""

from httpx import AsyncClient


async def test_a_safe_method_is_never_checked(client: AsyncClient):
    response = await client.get("/admin/login")
    assert response.status_code in (503, 302, 307)


async def test_an_unsafe_method_without_origin_or_referer_is_refused(client: AsyncClient):
    response = await client.post("/admin/logout")
    assert response.status_code == 403


async def test_an_unsafe_method_with_a_mismatched_origin_is_refused(client: AsyncClient):
    response = await client.post("/admin/logout", headers={"Origin": "https://evil.example"})
    assert response.status_code == 403


async def test_an_unsafe_method_with_a_matching_origin_is_accepted(client: AsyncClient):
    # follow_redirects=False : ce qu'on vérifie, c'est que la garde CSRF laisse passer jusqu'à la
    # redirection normale de /admin/logout (303), pas ce qui se passe ensuite sur /admin/login.
    response = await client.post(
        "/admin/logout", headers={"Origin": "http://test"}, follow_redirects=False
    )
    assert response.status_code == 303


async def test_a_referer_is_accepted_when_origin_is_absent(client: AsyncClient):
    response = await client.post(
        "/admin/logout", headers={"Referer": "http://test/admin"}, follow_redirects=False
    )
    assert response.status_code == 303
