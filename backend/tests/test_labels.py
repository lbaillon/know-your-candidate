from datetime import date

from kyc_api.labels import (
    candidate_statut_label,
    groupe_label,
    mandat_label,
    mise_au_point_label,
    rattachement_label,
    vote_position_label,
)


def test_candidate_statut_label_uses_the_imposed_wording() -> None:
    assert candidate_statut_label("declare") == "candidature déclarée"
    assert candidate_statut_label("pressenti") == "candidature pressentie"
    assert candidate_statut_label("retire") == "candidature retirée"


def test_an_unknown_statut_degrades_to_itself_rather_than_crashing() -> None:
    assert candidate_statut_label("ne_sait_pas") == "ne_sait_pas"


def test_groupe_label_for_an_ended_mandate() -> None:
    label = groupe_label(
        libelle="La France insoumise",
        debut=date(2017, 6, 21),
        fin=date(2022, 6, 21),
        non_inscrit=False,
    )
    assert label == "a siégé au groupe La France insoumise du 21/06/2017 au 21/06/2022"


def test_groupe_label_for_an_ongoing_mandate() -> None:
    label = groupe_label(
        libelle="La France insoumise", debut=date(2022, 6, 22), fin=None, non_inscrit=False
    )
    assert label == "a siégé au groupe La France insoumise depuis le 22/06/2022"


def test_groupe_label_never_says_a_siege_for_a_non_inscrit_mandate() -> None:
    label = groupe_label(libelle="Non inscrits", debut=date(2024, 1, 1), fin=None, non_inscrit=True)
    assert "a siégé" not in label
    assert label == "non-inscrit·e depuis le 01/01/2024"


def test_rattachement_label_never_says_membre_de() -> None:
    label = rattachement_label(libelle="Parti socialiste", debut=date(2015, 12, 1))
    assert "membre de" not in label
    assert (
        label
        == "rattaché·e au parti Parti socialiste au titre du financement de la vie politique, "
        "déclaration du 01/12/2015"
    )


def test_mandat_label_includes_the_legislature() -> None:
    label = mandat_label(legislature=17, debut=date(2024, 7, 18), fin=None)
    assert label == "député·e (17e législature) depuis le 18/07/2024"


def test_mandat_label_without_a_legislature_omits_the_parenthesis() -> None:
    label = mandat_label(legislature=None, debut=date(2024, 7, 18), fin=date(2025, 1, 1))
    assert label == "député·e du 18/07/2024 au 01/01/2025"


def test_vote_position_label_for_pour_contre_abstention() -> None:
    assert vote_position_label("pour") == "a voté pour"
    assert vote_position_label("contre") == "a voté contre"
    assert vote_position_label("abstention") == "s'est abstenu·e"


def test_vote_position_label_for_a_known_non_vote_cause() -> None:
    assert (
        vote_position_label("non_votant", "PSE")
        == "n'a pas pris part au vote : présidait la séance"
    )
    assert (
        vote_position_label("non_votant", "PAN")
        == "n'a pas pris part au vote : présidait l'Assemblée nationale"
    )
    assert (
        vote_position_label("non_votant", "MG")
        == "n'a pas pris part au vote : membre du Gouvernement"
    )


def test_vote_position_label_for_a_non_vote_without_a_known_cause_stays_uninterpreted() -> None:
    assert vote_position_label("non_votant", None) == "n'a pas pris part au vote"


def test_vote_position_label_never_crashes_on_an_unknown_cause_code() -> None:
    assert vote_position_label("non_votant", "CODE_INEDIT") == "n'a pas pris part au vote"


def test_mise_au_point_label_for_pour_and_contre() -> None:
    assert mise_au_point_label("pour") == "a déclaré après le scrutin avoir voulu voter pour"
    assert mise_au_point_label("contre") == "a déclaré après le scrutin avoir voulu voter contre"


def test_mise_au_point_label_for_abstention() -> None:
    assert mise_au_point_label("abstention") == "a déclaré après le scrutin avoir voulu s'abstenir"


def test_mise_au_point_label_for_non_votant_forms() -> None:
    expected = "a déclaré après le scrutin avoir voulu ne pas prendre part au vote"
    assert mise_au_point_label("non_votant") == expected
    assert mise_au_point_label("non_votant_volontaire") == expected
