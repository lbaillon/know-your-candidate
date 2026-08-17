from kyc_api.photos import thumbnail_url


def test_absent_commons_file_yields_no_url() -> None:
    assert thumbnail_url(None) is None


def test_blank_commons_file_yields_no_url() -> None:
    assert thumbnail_url("   ") is None


def test_turns_spaces_into_underscores() -> None:
    assert (
        thumbnail_url("Jean Dupont 2024.jpg")
        == "https://commons.wikimedia.org/wiki/Special:FilePath/Jean_Dupont_2024.jpg?width=400"
    )


def test_percent_encodes_accented_characters() -> None:
    assert (
        thumbnail_url("Mélenchon.jpg")
        == "https://commons.wikimedia.org/wiki/Special:FilePath/M%C3%A9lenchon.jpg?width=400"
    )


def test_percent_encodes_apostrophes() -> None:
    assert (
        thumbnail_url("Chloé d'Estaing.jpg")
        == "https://commons.wikimedia.org/wiki/Special:FilePath/Chlo%C3%A9_d%27Estaing.jpg?width=400"
    )
