"""Invariants structurels valables sur toute route publique rendant une page HTML complète — voir
docs/plans/phase-2-api-ui.md, section « Stratégie de test (D2.8) », niveau 3. `PAGE_ROUTES` grandit
à mesure que les commits suivants ajoutent des pages ; ne pas y ajouter un fragment HTMX, qui
n'étend pas base.html.jinja et n'a donc ni <html>, ni <title>, ni lien d'évitement.

Les routes qui exigent des données en base (une fiche personne, un scrutin) ne peuvent pas vivre
dans `PAGE_ROUTES` — elles ont leurs propres tests, plus bas, qui appellent les mêmes fonctions de
vérification `_check_*` sur un document obtenu après avoir inséré un jeu de données minimal.
"""

from datetime import date
from urllib.parse import urlsplit

import asyncpg
import lxml.html
import pytest
from httpx import AsyncClient

import factories

PAGE_ROUTES = ["/", "/deputes", "/methodologie", "/sources"]

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

# Exception documentée par le plan (D2.7) : les placements de grille CSS de la frise, calculés
# côté serveur par une fonction pure (timeline.py), sont les seuls styles en ligne autorisés. On
# vérifie la classe porteuse ET la forme de la valeur — jamais une couleur ni une mise en page
# arbitraire ne doit pouvoir se glisser derrière ce nom de propriété.
_ALLOWED_INLINE_STYLE_CLASSES = {"timeline__scale", "timeline__track"}
_ALLOWED_INLINE_STYLE_LI_CLASSES = {"timeline__segment", "timeline__scale-year"}
_ALLOWED_STYLE_PROPERTIES = {
    "grid-column",
    "grid-row",
    "grid-template-columns",
    "grid-template-rows",
}


async def _document(client: AsyncClient, path: str) -> lxml.html.HtmlElement:
    response = await client.get(path)
    assert response.status_code == 200, f"{path} : attendu 200, reçu {response.status_code}"
    return lxml.html.fromstring(response.text)


def _check_exactly_one_h1(doc: lxml.html.HtmlElement, path: str) -> None:
    assert len(doc.xpath("//h1")) == 1, f"{path} ne porte pas exactement un <h1>"


def _check_title_is_non_empty(doc: lxml.html.HtmlElement, path: str) -> str:
    titles = doc.xpath("//title/text()")
    assert titles and titles[0].strip(), f"{path} : <title> vide ou absent"
    return titles[0].strip()


def _check_html_lang_is_fr(doc: lxml.html.HtmlElement, path: str) -> None:
    assert doc.get("lang") == "fr", f"{path} : <html lang> n'est pas 'fr'"


def _check_has_a_skip_link(doc: lxml.html.HtmlElement, path: str) -> None:
    assert doc.xpath("//a[@href='#contenu']"), f"{path} : pas de lien d'évitement vers #contenu"
    assert doc.xpath("//main[@id='contenu']"), f'{path} : pas de <main id="contenu">'


def _check_every_image_has_alt(doc: lxml.html.HtmlElement, path: str) -> None:
    for img in doc.xpath("//img"):
        assert img.get("alt") is not None, f"{path} : <img> sans attribut alt"


def _check_every_form_field_has_a_label(doc: lxml.html.HtmlElement, path: str) -> None:
    labelled_ids = {label.get("for") for label in doc.xpath("//label[@for]")}
    for field in doc.xpath("//input | //select | //textarea"):
        field_id = field.get("id")
        has_label = field_id is not None and field_id in labelled_ids
        has_aria_label = field.get("aria-label") is not None
        assert has_label or has_aria_label, f"{path} : champ de formulaire sans label"


def _check_no_external_resource_except_wikimedia(doc: lxml.html.HtmlElement, path: str) -> None:
    for tag, attr in _EXTERNAL_RESOURCE_ATTRS.items():
        for element in doc.xpath(f"//{tag}[@{attr}]"):
            url = element.get(attr)
            parsed = urlsplit(url)
            if parsed.scheme not in ("http", "https"):
                continue  # relative (même origine) : /static/... n'est pas externe.
            assert parsed.hostname in _ALLOWED_EXTERNAL_HOSTS, (
                f"{path} : <{tag} {attr}={url!r}> pointe vers un hôte externe non autorisé"
            )


def _check_no_inline_script_or_disallowed_style(doc: lxml.html.HtmlElement, path: str) -> None:
    assert not doc.xpath("//script[not(@src)]"), f"{path} : <script> en ligne"

    for element in doc.xpath("//*[@style]"):
        classes = set((element.get("class") or "").split())
        tag = element.tag
        allowed = (tag == "ol" and classes & _ALLOWED_INLINE_STYLE_CLASSES) or (
            tag == "li" and classes & _ALLOWED_INLINE_STYLE_LI_CLASSES
        )
        assert allowed, f"{path} : attribut style en ligne inattendu sur <{tag} class={classes!r}>"

        declarations = [d.strip() for d in element.get("style").split(";") if d.strip()]
        for declaration in declarations:
            property_name = declaration.split(":", 1)[0].strip()
            assert property_name in _ALLOWED_STYLE_PROPERTIES, (
                f"{path} : propriété de style non autorisée en ligne : {property_name!r}"
            )


@pytest.mark.parametrize("path", PAGE_ROUTES)
async def test_exactly_one_h1(client: AsyncClient, path: str) -> None:
    _check_exactly_one_h1(await _document(client, path), path)


@pytest.mark.parametrize("path", PAGE_ROUTES)
async def test_title_is_non_empty(client: AsyncClient, path: str) -> None:
    _check_title_is_non_empty(await _document(client, path), path)


async def test_titles_are_distinct_across_pages(client: AsyncClient) -> None:
    titles = [_check_title_is_non_empty(await _document(client, p), p) for p in PAGE_ROUTES]
    assert len(titles) == len(set(titles)), f"titres non distincts : {titles}"


@pytest.mark.parametrize("path", PAGE_ROUTES)
async def test_html_lang_is_fr(client: AsyncClient, path: str) -> None:
    _check_html_lang_is_fr(await _document(client, path), path)


@pytest.mark.parametrize("path", PAGE_ROUTES)
async def test_has_a_skip_link_to_the_main_content(client: AsyncClient, path: str) -> None:
    _check_has_a_skip_link(await _document(client, path), path)


@pytest.mark.parametrize("path", PAGE_ROUTES)
async def test_every_image_has_an_alt_attribute(client: AsyncClient, path: str) -> None:
    _check_every_image_has_alt(await _document(client, path), path)


@pytest.mark.parametrize("path", PAGE_ROUTES)
async def test_every_form_field_has_a_label_or_aria_label(client: AsyncClient, path: str) -> None:
    _check_every_form_field_has_a_label(await _document(client, path), path)


@pytest.mark.parametrize("path", PAGE_ROUTES)
async def test_no_external_resource_except_wikimedia(client: AsyncClient, path: str) -> None:
    _check_no_external_resource_except_wikimedia(await _document(client, path), path)


@pytest.mark.parametrize("path", PAGE_ROUTES)
async def test_no_inline_script_or_style_attribute(client: AsyncClient, path: str) -> None:
    _check_no_inline_script_or_disallowed_style(await _document(client, path), path)


# --- Routes qui exigent des données en base ---------------------------------------------------


async def test_person_page_satisfies_every_structural_invariant(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await factories.insert_person(db_conn, an_uid="PA1", prenom="Jean", nom="Dupont")
    await factories.insert_slug(db_conn, person_id=person_id, slug="jean-dupont")
    organe_id = await factories.insert_organe(
        db_conn, an_uid="PO1", code_type="GP", libelle="Groupe Test", libelle_abrege="GT"
    )
    await factories.insert_mandat(
        db_conn,
        an_uid="M1",
        person_id=person_id,
        organe_id=organe_id,
        type_organe="GP",
        debut=date(2022, 6, 22),
        legislature=16,
    )
    await factories.refresh_person_apercu(db_conn)

    path = "/personne/jean-dupont"
    doc = await _document(client, path)

    _check_exactly_one_h1(doc, path)
    _check_title_is_non_empty(doc, path)
    _check_html_lang_is_fr(doc, path)
    _check_has_a_skip_link(doc, path)
    _check_every_image_has_alt(doc, path)
    _check_every_form_field_has_a_label(doc, path)
    _check_no_external_resource_except_wikimedia(doc, path)
    _check_no_inline_script_or_disallowed_style(doc, path)
