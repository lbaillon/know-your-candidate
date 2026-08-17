import pytest

from kyc_api.documents import render_document


def test_renders_methodology_heading_as_the_single_h1() -> None:
    html = render_document("methodology.md")

    assert "<h1>Méthodologie</h1>" in html


def test_renders_a_markdown_table_as_an_html_table() -> None:
    html = render_document("methodology.md")

    # § 3 « Comment on lit une position de vote » est une table Markdown dans le document source.
    assert "<table>" in html


def test_unknown_document_raises() -> None:
    with pytest.raises(FileNotFoundError):
        render_document("does-not-exist.md")
