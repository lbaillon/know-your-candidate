"""Placement de la frise des appartenances — voir docs/plans/phase-2-api-ui.md, section « La
frise : spécification du calcul » (D2.7). Fonction pure, aucun accès à la base : `today` est
toujours injecté, jamais lu ici, pour qu'une frise reste testable à une date fixe.

Ce module ne calcule que la géométrie (colonnes, rangées) : le texte de chaque segment est déjà
formé par l'appelant (voir labels.py) — c'est ce texte, en clair dans le HTML, qui rend la frise
accessible (pas un `aria-label`).
"""

from collections.abc import Sequence
from dataclasses import dataclass, field
from datetime import date

# Ordre fixe des pistes (règle 6) : indépendant des données, une piste sans segment reste à sa
# place plutôt que de disparaître ou de se réordonner.
GP = "GP"
PARPOL = "PARPOL"
ASSEMBLEE = "ASSEMBLEE"
PISTE_ORDER = (GP, PARPOL, ASSEMBLEE)


@dataclass(frozen=True)
class Segment:
    """Une entrée à placer sur la frise. `key` ne sert qu'au départage déterministe du placement
    glouton (l'`an_uid` du mandat, voir règle 5 — la leçon de F7 en phase 1.1) : deux segments
    identiques en tout sauf leur `an_uid` doivent être ordonnés de façon stable.
    """

    piste: str
    key: str
    debut: date
    fin: date | None
    label: str


@dataclass(frozen=True)
class PlacedSegment:
    segment: Segment
    column_start: int
    column_end: int
    row: int


@dataclass(frozen=True)
class Piste:
    key: str
    segments: list[PlacedSegment] = field(default_factory=list)
    row_count: int = 0


@dataclass(frozen=True)
class YearMarker:
    year: int
    column_start: int
    column_end: int


@dataclass(frozen=True)
class Timeline:
    pistes: list[Piste] = field(default_factory=list)
    years: list[YearMarker] = field(default_factory=list)
    total_columns: int = 0


def _months_between(start: date, end: date) -> int:
    """Nombre de mois entiers entre deux dates, comptés sur année/mois seuls (le jour n'entre pas
    en jeu : l'unité de colonne est le mois, règle 1).
    """
    return (end.year - start.year) * 12 + (end.month - start.month)


def _column(domain_start: date, d: date) -> int:
    """Règle 2 : `colonne(date) = mois_entre(domaine.debut, date) + 1` (CSS indexe à 1)."""
    return _months_between(domain_start, d) + 1


def _first_of_month(d: date) -> date:
    return date(d.year, d.month, 1)


def _first_of_next_month(d: date) -> date:
    if d.month == 12:
        return date(d.year + 1, 1, 1)
    return date(d.year, d.month + 1, 1)


def _year_markers(domain_start: date, total_columns: int) -> list[YearMarker]:
    """Règle 9 : une rangée d'échelle donne les années, chacune couvrant ses colonnes."""
    markers: list[YearMarker] = []
    current_year: int | None = None
    current_start = 1
    for index in range(total_columns):
        column = index + 1
        total_months = (domain_start.month - 1) + index
        year = domain_start.year + total_months // 12
        if year != current_year:
            if current_year is not None:
                markers.append(
                    YearMarker(year=current_year, column_start=current_start, column_end=column)
                )
            current_year = year
            current_start = column
    if current_year is not None:
        markers.append(
            YearMarker(year=current_year, column_start=current_start, column_end=total_columns + 1)
        )
    return markers


def _place_piste(
    segments: Sequence[Segment], *, domain_start: date, total_columns: int
) -> tuple[list[PlacedSegment], int]:
    """Règle 5 : placement glouton, trié par début croissant puis fin croissante puis `key`,
    chaque segment sur la première rangée dont l'occupation ne le chevauche pas.
    """
    ordered = sorted(segments, key=lambda s: (s.debut, s.fin or date.max, s.key))
    row_ends: list[int] = []  # colonne de fin (exclusive) déjà occupée, par rangée
    placed: list[PlacedSegment] = []

    for segment in ordered:
        column_start = _column(domain_start, segment.debut)
        if segment.fin is None:
            # Règle 4 : un mandat en cours court jusqu'à la dernière colonne du domaine.
            column_end = total_columns + 1
        else:
            # Règle 3 : un segment occupe au minimum une colonne (mandat d'un seul jour compris).
            column_end = max(_column(domain_start, segment.fin), column_start + 1)

        row = next((i for i, end in enumerate(row_ends) if end <= column_start), None)
        if row is None:
            row = len(row_ends)
            row_ends.append(column_end)
        else:
            row_ends[row] = column_end

        placed.append(
            PlacedSegment(
                segment=segment, column_start=column_start, column_end=column_end, row=row
            )
        )

    return placed, len(row_ends)


def build_timeline(segments: Sequence[Segment], *, today: date) -> Timeline:
    if not segments:
        # Règle 8 : aucun segment, aucune frise — le gabarit affiche l'état vide, pas une grille
        # de zéro colonne.
        return Timeline()

    domain_start = _first_of_month(min(segment.debut for segment in segments))
    latest = max(segment.fin if segment.fin is not None else today for segment in segments)
    domain_end = _first_of_next_month(latest)
    total_columns = _months_between(domain_start, domain_end)

    pistes = []
    for piste_key in PISTE_ORDER:
        piste_segments = [s for s in segments if s.piste == piste_key]
        placed, row_count = _place_piste(
            piste_segments, domain_start=domain_start, total_columns=total_columns
        )
        pistes.append(Piste(key=piste_key, segments=placed, row_count=row_count))
    years = _year_markers(domain_start, total_columns)

    return Timeline(pistes=pistes, years=years, total_columns=total_columns)
