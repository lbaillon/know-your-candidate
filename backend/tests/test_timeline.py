from datetime import date

from kyc_api.timeline import ASSEMBLEE, GP, PARPOL, Segment, build_timeline


def seg(piste=GP, key="M1", debut=date(2020, 1, 1), fin=None, label="segment") -> Segment:
    return Segment(piste=piste, key=key, debut=debut, fin=fin, label=label)


def test_empty_segments_yield_an_empty_timeline() -> None:
    timeline = build_timeline([], today=date(2026, 8, 17))

    assert timeline.total_columns == 0
    assert timeline.pistes == []
    assert timeline.years == []


def test_domain_starts_at_the_first_of_the_earliest_month() -> None:
    timeline = build_timeline(
        [seg(debut=date(2020, 3, 15), fin=date(2020, 4, 1))], today=date(2026, 8, 17)
    )

    gp = next(p for p in timeline.pistes if p.key == GP)
    assert gp.segments[0].column_start == 1


def test_a_single_day_mandate_occupies_exactly_one_column() -> None:
    timeline = build_timeline(
        [seg(debut=date(2020, 3, 10), fin=date(2020, 3, 10))], today=date(2026, 8, 17)
    )

    gp = next(p for p in timeline.pistes if p.key == GP)
    placed = gp.segments[0]
    assert placed.column_end - placed.column_start == 1


def test_an_ongoing_mandate_reaches_the_last_column() -> None:
    timeline = build_timeline([seg(debut=date(2020, 1, 1), fin=None)], today=date(2020, 4, 15))

    gp = next(p for p in timeline.pistes if p.key == GP)
    placed = gp.segments[0]
    # Domaine : janvier -> mai (mois suivant avril, où `today` tombe) = 4 colonnes.
    assert timeline.total_columns == 4
    assert placed.column_end == timeline.total_columns + 1


def test_an_ongoing_mandate_never_gets_an_invented_end_date() -> None:
    """Ce test documente une garantie de conception : `fin=None` reste `None` en sortie, le
    gabarit décide d'écrire « depuis le … » — `build_timeline` n'invente jamais de date de fin."""
    timeline = build_timeline([seg(debut=date(2020, 1, 1), fin=None)], today=date(2020, 4, 15))

    gp = next(p for p in timeline.pistes if p.key == GP)
    assert gp.segments[0].segment.fin is None


def test_columns_span_two_months_when_the_mandate_crosses_a_month_boundary() -> None:
    timeline = build_timeline(
        [seg(debut=date(2020, 1, 15), fin=date(2020, 2, 1))], today=date(2026, 8, 17)
    )

    gp = next(p for p in timeline.pistes if p.key == GP)
    placed = gp.segments[0]
    assert placed.column_start == 1
    assert placed.column_end == 2


def test_overlapping_segments_in_the_same_piste_go_on_two_rows() -> None:
    a = seg(key="A", debut=date(2020, 1, 1), fin=date(2020, 6, 1))
    b = seg(key="B", debut=date(2020, 3, 1), fin=date(2020, 9, 1))
    timeline = build_timeline([a, b], today=date(2026, 8, 17))

    gp = next(p for p in timeline.pistes if p.key == GP)
    assert gp.row_count == 2
    rows = {placed.segment.key: placed.row for placed in gp.segments}
    assert rows["A"] != rows["B"]


def test_non_overlapping_segments_share_a_row() -> None:
    a = seg(key="A", debut=date(2020, 1, 1), fin=date(2020, 3, 1))
    b = seg(key="B", debut=date(2020, 3, 1), fin=date(2020, 6, 1))
    timeline = build_timeline([a, b], today=date(2026, 8, 17))

    gp = next(p for p in timeline.pistes if p.key == GP)
    assert gp.row_count == 1


def test_placement_order_is_deterministic_on_ties() -> None:
    """Règle 5 : tri par début croissant, puis fin croissante, puis `key` — la leçon de F7."""
    a = seg(key="B", debut=date(2020, 1, 1), fin=date(2020, 6, 1))
    b = seg(key="A", debut=date(2020, 1, 1), fin=date(2020, 6, 1))
    timeline = build_timeline([a, b], today=date(2026, 8, 17))

    gp = next(p for p in timeline.pistes if p.key == GP)
    # Même début, même fin : "A" (key le plus petit) doit être traité en premier, donc rangée 0.
    by_key = {placed.segment.key: placed.row for placed in gp.segments}
    assert by_key["A"] == 0
    assert by_key["B"] == 1


def test_the_three_pistes_are_always_present_in_a_fixed_order() -> None:
    timeline = build_timeline([seg(piste=GP)], today=date(2026, 8, 17))

    assert [p.key for p in timeline.pistes] == [GP, PARPOL, ASSEMBLEE]
    assemblee = next(p for p in timeline.pistes if p.key == ASSEMBLEE)
    assert assemblee.segments == []
    assert assemblee.row_count == 0


def test_the_ps_to_lfi_recette_case_places_two_contiguous_group_segments_without_overlap() -> None:
    """Cas de recette du cahier des charges : une personne passée du PS à LFI, mandats GP
    consécutifs et non chevauchants (la normalisation d'ingestion garantit l'absence de
    chevauchement inter-organe pour ce cas réel)."""
    ps = seg(piste=GP, key="M-PS", debut=date(2017, 6, 21), fin=date(2018, 8, 30), label="PS")
    lfi = seg(piste=GP, key="M-LFI", debut=date(2018, 8, 31), fin=None, label="LFI")
    timeline = build_timeline([ps, lfi], today=date(2020, 1, 1))

    gp = next(p for p in timeline.pistes if p.key == GP)
    assert gp.row_count == 1, "deux mandats consécutifs ne se chevauchent pas : une seule rangée"
    by_key = {placed.segment.key: placed for placed in gp.segments}
    assert by_key["M-PS"].column_end <= by_key["M-LFI"].column_start + 1


def test_year_markers_cover_their_columns() -> None:
    timeline = build_timeline(
        [seg(debut=date(2019, 11, 1), fin=date(2020, 2, 1))], today=date(2026, 8, 17)
    )

    assert [marker.year for marker in timeline.years] == [2019, 2020]
    total_span = sum(marker.column_end - marker.column_start for marker in timeline.years)
    assert total_span == timeline.total_columns
    assert timeline.years[0].column_start == 1
    assert timeline.years[-1].column_end == timeline.total_columns + 1


def test_a_year_entirely_within_the_domain_is_not_skipped() -> None:
    timeline = build_timeline(
        [seg(debut=date(2018, 1, 1), fin=date(2020, 1, 1))], today=date(2026, 8, 17)
    )

    assert [marker.year for marker in timeline.years] == [2018, 2019, 2020]
