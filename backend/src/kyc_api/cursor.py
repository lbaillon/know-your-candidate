"""Curseur de pagination de la liste des votes — voir docs/plans/phase-2-api-ui.md, section
« Liste des votes : pagination et filtres ». Fonction pure, testée seule (D2.8) : le tri
(`date_scrutin DESC, scrutin_id DESC`) et le format du curseur (`AAAA-MM-JJ,{scrutin_id}`) sont
définis ici, une seule fois, partagés par la page et l'API.
"""

from datetime import date


class InvalidCursor(ValueError):
    pass


def parse_cursor(raw: str | None) -> tuple[date, int] | None:
    """`None` en entrée (pas de curseur) rend `None`. Une valeur mal formée lève `InvalidCursor` :
    c'est à l'appelant de décider comment dégrader (voir routers/pages.py — silencieusement vers
    la première page, un curseur n'a pas de sens à faire échouer une page publique).
    """
    if raw is None:
        return None
    parts = raw.split(",")
    if len(parts) != 2:
        raise InvalidCursor(f"curseur invalide : {raw!r}")
    date_part, id_part = parts
    try:
        parsed_date = date.fromisoformat(date_part)
        scrutin_id = int(id_part)
    except ValueError as exc:
        raise InvalidCursor(f"curseur invalide : {raw!r}") from exc
    return parsed_date, scrutin_id


def format_cursor(date_scrutin: date, scrutin_id: int) -> str:
    return f"{date_scrutin.isoformat()},{scrutin_id}"
