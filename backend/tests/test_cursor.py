from datetime import date

import pytest

from kyc_api.cursor import InvalidCursor, format_cursor, parse_cursor


def test_parse_cursor_returns_none_for_no_cursor() -> None:
    assert parse_cursor(None) is None


def test_parse_cursor_splits_date_and_id() -> None:
    assert parse_cursor("2023-03-14,4321") == (date(2023, 3, 14), 4321)


def test_format_cursor_matches_the_documented_format() -> None:
    assert format_cursor(date(2023, 3, 14), 4321) == "2023-03-14,4321"


def test_format_then_parse_round_trips() -> None:
    original = (date(2020, 1, 1), 1)
    assert parse_cursor(format_cursor(*original)) == original


@pytest.mark.parametrize(
    "raw",
    [
        "",
        "no-comma-here",
        "2023-03-14",
        "2023-03-14,4321,extra",
        "not-a-date,4321",
        "2023-03-14,not-an-int",
    ],
)
def test_parse_cursor_rejects_malformed_input(raw: str) -> None:
    with pytest.raises(InvalidCursor):
        parse_cursor(raw)
