"""SQL sur `person` / `person_apercu` — accueil, annuaire et fiche personne. Tout le SQL vit ici,
aucune requête ailleurs (CLAUDE.md) ; les gabarits et l'API consomment les mêmes modèles Pydantic
(D2.10).
"""

from dataclasses import dataclass
from datetime import date, timedelta
from typing import Any
from urllib.parse import urlencode

from asyncpg import Record

from kyc_api.cursor import format_cursor
from kyc_api.db import Queryable
from kyc_api.labels import groupe_label, mandat_label, rattachement_label
from kyc_api.schemas.common import Pagination, Source
from kyc_api.schemas.person import CandidateStatus, GroupeApercu, PersonApercu, PersonDetail
from kyc_api.schemas.vote import VoteRecent
from kyc_api.timeline import Segment

# Page suffisamment grande pour tenir l'annuaire sur peu de pages (~3 100 personnes ingérées),
# assez petite pour rester lisible.
DIRECTORY_PAGE_SIZE = 30

# Nombre de votes affichés sur la fiche personne — la liste complète a sa propre page paginée
# (voir plan d'exécution, section « Liste des votes »).
RECENT_VOTES_LIMIT = 10

# Taille d'une page de la liste complète des votes (pagination par curseur).
VOTES_PAGE_SIZE = 50

# Licence AN, rappelée à chaque bloc de source (CLAUDE.md, « chaque bloc affiche sa source »).
AN_LICENCE = "Licence Ouverte (Etalab)"


def person_apercu_fields(row: Record) -> dict[str, Any]:
    """Champs communs à `PersonApercu` et `CandidateCard`, extraits d'une ligne de
    `person_apercu` (ou d'une jointure qui la contient en entier via `pa.*`).

    « Dernier groupe connu » et non « actuel » (voir migration 0005) : `dernier_groupe_period` est
    un `daterange` borne supérieure exclusive (`[)`), donc `fin` affichée = borne - 1 jour.
    """
    groupe = None
    if row["dernier_groupe_id"] is not None:
        period = row["dernier_groupe_period"]
        fin = period.upper
        if fin is not None:
            fin = fin - timedelta(days=1)
        groupe = GroupeApercu(
            organe_id=row["dernier_groupe_id"],
            libelle=row["dernier_groupe_libelle"],
            libelle_abrege=row["dernier_groupe_abrege"],
            non_inscrit=row["dernier_groupe_non_inscrit"],
            debut=period.lower,
            fin=fin,
        )
    return {
        "person_id": row["person_id"],
        "slug": row["slug"],
        "civilite": row["civilite"],
        "prenom": row["prenom"],
        "nom": row["nom"],
        "date_deces": row["date_deces"],
        "photo_url": row["photo_url"],
        "commons_file": row["commons_file"],
        "licence": row["licence"],
        "licence_url": row["licence_url"],
        "auteur": row["auteur"],
        "votes_total": row["votes_total"],
        "premier_vote": row["premier_vote"],
        "dernier_vote": row["dernier_vote"],
        "groupe": groupe,
        "legislatures": list(row["legislatures"]),
        "est_candidat": row["est_candidat"],
    }


def person_apercu_from_row(row: Record) -> PersonApercu:
    return PersonApercu(**person_apercu_fields(row))


_DIRECTORY_WHERE = """
    pa.slug IS NOT NULL
    AND (
        $1::text IS NULL
        OR (coalesce(pa.prenom, '') || ' ' || coalesce(pa.nom, '')) ILIKE '%' || $1 || '%'
    )
    AND ($2::smallint IS NULL OR $2 = ANY(pa.legislatures))
"""


async def list_directory(
    pool: Queryable, *, q: str | None, legislature: int | None, page: int
) -> tuple[list[PersonApercu], Pagination]:
    page = max(page, 1)
    offset = (page - 1) * DIRECTORY_PAGE_SIZE

    rows = await pool.fetch(
        f"""
        SELECT pa.*
        FROM person_apercu pa
        WHERE {_DIRECTORY_WHERE}
        ORDER BY pa.nom, pa.prenom, pa.person_id
        LIMIT $3 OFFSET $4
        """,
        q,
        legislature,
        DIRECTORY_PAGE_SIZE,
        offset,
    )
    total = await pool.fetchval(
        f"""
        SELECT count(*)
        FROM person_apercu pa
        WHERE {_DIRECTORY_WHERE}
        """,
        q,
        legislature,
    )
    assert isinstance(total, int)

    persons = [person_apercu_from_row(row) for row in rows]
    return persons, Pagination(page=page, page_size=DIRECTORY_PAGE_SIZE, total=total)


@dataclass(frozen=True)
class SlugResolution:
    person_id: int
    current_slug: str
    is_current: bool


async def resolve_slug(pool: Queryable, slug: str) -> SlugResolution | None:
    """`None` si le slug n'a jamais existé (404). Si la personne visée n'a, pour une raison ou une
    autre, plus aucun slug courant, la jointure interne sur `cur.is_current` ne rend pas de ligne
    non plus : un slug sans page à servir n'est pas distinguable d'un slug inconnu pour
    l'appelant, et c'est le comportement voulu (aucune URL de secours à inventer).
    """
    row = await pool.fetchrow(
        """
        SELECT ps.person_id, ps.is_current, cur.slug AS current_slug
        FROM person_slug ps
        JOIN person_slug cur ON cur.person_id = ps.person_id AND cur.is_current
        WHERE ps.slug = $1
        """,
        slug,
    )
    if row is None:
        return None
    return SlugResolution(
        person_id=row["person_id"],
        current_slug=row["current_slug"],
        is_current=row["is_current"],
    )


async def get_person_detail(pool: Queryable, person_id: int) -> PersonDetail | None:
    """La source d'une personne n'est pas dans un champ de `person` (contrairement à un scrutin
    ou une photo) : on va la chercher dans `source_document`, servie par
    `source_document_courant_idx` (voir plan d'exécution, « Les sources affichées »). Absente pour
    une personne qui n'a jamais siégé : elle n'a alors aucun `an_uid`, donc aucun document AN.
    """
    row = await pool.fetchrow(
        """
        SELECT p.id AS person_id, s.slug, p.civilite, p.prenom, p.nom, p.date_deces,
               p.an_uid, p.wikidata_qid,
               ph.url AS photo_url, ph.commons_file, ph.licence, ph.licence_url, ph.auteur,
               c.statut, c.source_url AS candidature_source_url, c.source_date, c.note,
               sd.url AS source_document_url, sd.fetched_at AS source_document_fetched_at
        FROM person p
        LEFT JOIN person_slug s ON s.person_id = p.id AND s.is_current
        LEFT JOIN person_photo ph ON ph.person_id = p.id
        LEFT JOIN candidate c ON c.person_id = p.id
        LEFT JOIN LATERAL (
            SELECT url, fetched_at
            FROM source_document
            WHERE source = 'an_acteur' AND uid = p.an_uid
            ORDER BY fetched_at DESC
            LIMIT 1
        ) sd ON true
        WHERE p.id = $1
        """,
        person_id,
    )
    if row is None:
        return None

    candidature = None
    if row["statut"] is not None:
        candidature = CandidateStatus(
            statut=row["statut"],
            source_url=row["candidature_source_url"],
            source_date=row["source_date"],
            note=row["note"],
        )

    source = None
    if row["source_document_url"] is not None:
        source = Source(
            url=row["source_document_url"],
            fetched_at=row["source_document_fetched_at"],
            licence=AN_LICENCE,
        )

    return PersonDetail(
        person_id=row["person_id"],
        slug=row["slug"],
        civilite=row["civilite"],
        prenom=row["prenom"],
        nom=row["nom"],
        date_deces=row["date_deces"],
        an_uid=row["an_uid"],
        wikidata_qid=row["wikidata_qid"],
        photo_url=row["photo_url"],
        commons_file=row["commons_file"],
        licence=row["licence"],
        licence_url=row["licence_url"],
        auteur=row["auteur"],
        candidature=candidature,
        source=source,
    )


async def get_timeline_segments(pool: Queryable, person_id: int) -> list[Segment]:
    """Mandats `GP`, `PARPOL` et `ASSEMBLEE` d'une personne, transformés en `Segment` de la frise
    (timeline.py) : le texte de chaque segment est construit ici, une fois, via labels.py — pas
    dans le gabarit.
    """
    rows = await pool.fetch(
        """
        SELECT m.an_uid, m.type_organe, m.legislature,
               lower(m.period) AS debut, upper(m.period) AS fin_exclusive,
               o.libelle, o.libelle_abrege, o.is_non_inscrit
        FROM mandat m
        JOIN organe o ON o.id = m.organe_id
        WHERE m.person_id = $1 AND m.type_organe IN ('GP', 'PARPOL', 'ASSEMBLEE')
        ORDER BY m.period
        """,
        person_id,
    )

    segments = []
    for row in rows:
        debut = row["debut"]
        fin_exclusive = row["fin_exclusive"]
        fin = fin_exclusive - timedelta(days=1) if fin_exclusive is not None else None
        type_organe = row["type_organe"]

        if type_organe == "GP":
            label = groupe_label(
                libelle=row["libelle"], debut=debut, fin=fin, non_inscrit=row["is_non_inscrit"]
            )
        elif type_organe == "PARPOL":
            label = rattachement_label(libelle=row["libelle"], debut=debut)
        else:
            label = mandat_label(legislature=row["legislature"], debut=debut, fin=fin)

        segments.append(
            Segment(piste=type_organe, key=row["an_uid"], debut=debut, fin=fin, label=label)
        )

    return segments


async def get_recent_votes(
    pool: Queryable, person_id: int, *, limit: int = RECENT_VOTES_LIMIT
) -> list[VoteRecent]:
    rows = await pool.fetch(
        """
        SELECT sc.id AS scrutin_id, sc.an_uid AS scrutin_an_uid, sc.legislature, sc.numero,
               sc.titre, sc.date_scrutin, sc.chambre::text AS chambre,
               v.position::text AS position, v.cause_non_vote, v.par_delegation
        FROM vote v
        JOIN scrutin sc ON sc.id = v.scrutin_id
        WHERE v.person_id = $1
        ORDER BY sc.date_scrutin DESC, sc.id DESC
        LIMIT $2
        """,
        person_id,
        limit,
    )
    return [VoteRecent(**dict(row)) for row in rows]


@dataclass(frozen=True)
class VotesPage:
    votes: list[VoteRecent]
    next_cursor: str | None


async def list_votes(
    pool: Queryable,
    person_id: int,
    *,
    legislature: int | None,
    position: str | None,
    du: date | None,
    au: date | None,
    groupe: str | None,
    avant: tuple[date, int] | None,
) -> VotesPage:
    """`avant` est déjà décodé (voir cursor.parse_cursor) — cette fonction ne connaît que la paire
    `(date_scrutin, scrutin_id)`, jamais le format texte du curseur. `groupe` est l'`an_uid` de
    l'organe (pas son id interne) : c'est ce que porterait un lien externe.

    Un lot de `VOTES_PAGE_SIZE + 1` est demandé pour savoir s'il existe une page suivante sans
    requête `count(*)` séparée sur une table de 2,3 M de lignes.
    """
    rows = await pool.fetch(
        """
        SELECT sc.id AS scrutin_id, sc.an_uid AS scrutin_an_uid, sc.legislature, sc.numero,
               sc.titre, sc.date_scrutin, sc.chambre::text AS chambre,
               v.position::text AS position, v.cause_non_vote, v.par_delegation
        FROM vote v
        JOIN scrutin sc ON sc.id = v.scrutin_id
        LEFT JOIN organe o ON o.id = v.groupe_organe_id
        WHERE v.person_id = $1
          AND ($2::smallint IS NULL OR sc.legislature = $2)
          AND ($3::text IS NULL OR v.position::text = $3)
          AND ($4::date IS NULL OR sc.date_scrutin >= $4)
          AND ($5::date IS NULL OR sc.date_scrutin <= $5)
          AND ($6::text IS NULL OR o.an_uid = $6)
          AND ($7::date IS NULL OR (sc.date_scrutin, sc.id) < ($7, $8))
        ORDER BY sc.date_scrutin DESC, sc.id DESC
        LIMIT $9
        """,
        person_id,
        legislature,
        position,
        du,
        au,
        groupe,
        avant[0] if avant else None,
        avant[1] if avant else None,
        VOTES_PAGE_SIZE + 1,
    )

    has_more = len(rows) > VOTES_PAGE_SIZE
    page_rows = rows[:VOTES_PAGE_SIZE]
    votes = [VoteRecent(**dict(row)) for row in page_rows]

    next_cursor = None
    if has_more and page_rows:
        last = page_rows[-1]
        next_cursor = format_cursor(last["date_scrutin"], last["scrutin_id"])

    return VotesPage(votes=votes, next_cursor=next_cursor)


def votes_filter_query_string(
    *,
    legislature: int | None,
    position: str | None,
    du: date | None,
    au: date | None,
    groupe: str | None,
) -> str:
    """Les filtres actifs, prêts à préfixer `avant=...` dans un lien de pagination (page ou
    fragment) — écrit une seule fois plutôt que dans chaque gabarit qui construit ce lien.
    """
    params: dict[str, str] = {}
    if legislature is not None:
        params["legislature"] = str(legislature)
    if position is not None:
        params["position"] = position
    if du is not None:
        params["du"] = du.isoformat()
    if au is not None:
        params["au"] = au.isoformat()
    if groupe is not None:
        params["groupe"] = groupe
    query_string = urlencode(params)
    return f"{query_string}&" if query_string else ""
