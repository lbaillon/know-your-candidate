"""Invariants structurels valables sur toute route publique rendant une page HTML complète — voir
docs/plans/phase-2-api-ui.md, section « Stratégie de test (D2.8) », niveau 3. `PAGE_ROUTES` grandit
à mesure que les commits suivants ajoutent des pages ; ne pas y ajouter un fragment HTMX, qui
n'étend pas base.html.jinja et n'a donc ni <html>, ni <title>, ni lien d'évitement.
"""

from urllib.parse import urlsplit

import lxml.html
import pytest
from httpx import AsyncClient

PAGE_ROUTES = ["/", "/methodologie", "/sources"]

# tag -> attribut qui déclenche une requête au chargement de la page.
_EXTERNAL_RESOURCE_ATTRS = {
    "img": "src",
    "script": "src",
    "link": "href",
    "iframe": "src",
    "source": "src",
    "video": "src",
}
_ALLOWED_EXTERNAL_HOSTS = {"commons.wikimedia.org", "upload.wikimedia.org"}


async def _document(client: AsyncClient, path: str) -> lxml.html.HtmlElement:
    response = await client.get(path)
    assert response.status_code == 200, f"{path} : attendu 200, reçu {response.status_code}"
    return lxml.html.fromstring(response.text)


@pytest.mark.parametrize("path", PAGE_ROUTES)
async def test_exactly_one_h1(client: AsyncClient, path: str) -> None:
    doc = await _document(client, path)
    assert len(doc.xpath("//h1")) == 1, f"{path} ne porte pas exactement un <h1>"


@pytest.mark.parametrize("path", PAGE_ROUTES)
async def test_title_is_non_empty(client: AsyncClient, path: str) -> None:
    doc = await _document(client, path)
    titles = doc.xpath("//title/text()")
    assert titles and titles[0].strip(), f"{path} : <title> vide ou absent"


async def test_titles_are_distinct_across_pages(client: AsyncClient) -> None:
    titles = []
    for path in PAGE_ROUTES:
        doc = await _document(client, path)
        titles.append(doc.xpath("//title/text()")[0].strip())
    assert len(titles) == len(set(titles)), f"titres non distincts : {titles}"


@pytest.mark.parametrize("path", PAGE_ROUTES)
async def test_html_lang_is_fr(client: AsyncClient, path: str) -> None:
    doc = await _document(client, path)
    assert doc.get("lang") == "fr"


@pytest.mark.parametrize("path", PAGE_ROUTES)
async def test_has_a_skip_link_to_the_main_content(client: AsyncClient, path: str) -> None:
    doc = await _document(client, path)
    assert doc.xpath("//a[@href='#contenu']"), f"{path} : pas de lien d'évitement vers #contenu"
    assert doc.xpath("//main[@id='contenu']"), f'{path} : pas de <main id="contenu">'


@pytest.mark.parametrize("path", PAGE_ROUTES)
async def test_every_image_has_an_alt_attribute(client: AsyncClient, path: str) -> None:
    doc = await _document(client, path)
    for img in doc.xpath("//img"):
        assert img.get("alt") is not None, f"{path} : <img> sans attribut alt"


@pytest.mark.parametrize("path", PAGE_ROUTES)
async def test_every_form_field_has_a_label_or_aria_label(client: AsyncClient, path: str) -> None:
    doc = await _document(client, path)
    labelled_ids = {label.get("for") for label in doc.xpath("//label[@for]")}
    for field in doc.xpath("//input | //select | //textarea"):
        field_id = field.get("id")
        has_label = field_id is not None and field_id in labelled_ids
        has_aria_label = field.get("aria-label") is not None
        assert has_label or has_aria_label, f"{path} : champ de formulaire sans label"


@pytest.mark.parametrize("path", PAGE_ROUTES)
async def test_no_external_resource_except_wikimedia(client: AsyncClient, path: str) -> None:
    doc = await _document(client, path)
    for tag, attr in _EXTERNAL_RESOURCE_ATTRS.items():
        for element in doc.xpath(f"//{tag}[@{attr}]"):
            url = element.get(attr)
            parsed = urlsplit(url)
            if parsed.scheme not in ("http", "https"):
                continue  # relative (même origine) : /static/... n'est pas externe.
            assert parsed.hostname in _ALLOWED_EXTERNAL_HOSTS, (
                f"{path} : <{tag} {attr}={url!r}> pointe vers un hôte externe non autorisé"
            )


@pytest.mark.parametrize("path", PAGE_ROUTES)
async def test_no_inline_script_or_style_attribute(client: AsyncClient, path: str) -> None:
    doc = await _document(client, path)
    assert not doc.xpath("//script[not(@src)]"), f"{path} : <script> en ligne"
    # Exception explicite à venir avec la frise (grid-column calculé côté serveur, D2.7) : pas
    # encore de cas à ce stade de la phase.
    assert not doc.xpath("//*[@style]"), f"{path} : attribut style en ligne"
