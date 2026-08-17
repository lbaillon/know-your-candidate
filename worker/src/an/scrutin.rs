//! Parsing d'un scrutin — voir docs/plans/phase-1-ingestion.md, section « Job ingest_scrutins ».
//! Le JSON de l'AN nomme ses blocs nominatifs au pluriel (`pours`, `contres`, `abstentions`,
//! `nonVotants`) partout sauf sur les scrutins du Congrès, qui les nomment au singulier (`pour`,
//! `contre`, `abstention`, `nonVotant`) : un parseur qui ne connaît que le pluriel ingère zéro
//! vote sur ces scrutins sans lever d'erreur. Les deux graphies sont donc acceptées ici, et tout
//! autre nom de bloc est renvoyé comme anomalie plutôt qu'ignoré en silence.

use chrono::NaiveDate;
use serde_json::Value;

use super::json::{CollectionIter, booleen, collection, field, scalaire};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chambre {
    Assemblee,
    Congres,
}

impl Chambre {
    /// Valeur exacte de l'enum Postgres `chambre` (migration 0003).
    pub fn as_db_str(self) -> &'static str {
        match self {
            Chambre::Assemblee => "assemblee",
            Chambre::Congres => "congres",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VotePosition {
    Pour,
    Contre,
    Abstention,
    NonVotant,
}

impl VotePosition {
    /// Valeur exacte de l'enum Postgres `vote_position` (migration 0003).
    pub fn as_db_str(self) -> &'static str {
        match self {
            VotePosition::Pour => "pour",
            VotePosition::Contre => "contre",
            VotePosition::Abstention => "abstention",
            VotePosition::NonVotant => "non_votant",
        }
    }
}

pub struct ParsedVote {
    pub acteur_uid: String,
    pub mandat_uid: Option<String>,
    pub position: VotePosition,
    pub cause_non_vote: Option<String>,
    pub par_delegation: bool,
}

pub struct ParsedGroupeVentilation {
    pub organe_uid: Option<String>,
    pub nombre_membres: i32,
    pub position_majoritaire: Option<String>,
    pub pour: i32,
    pub contre: i32,
    pub abstentions: i32,
    pub non_votants: i32,
    pub votes: Vec<ParsedVote>,
    /// Noms de blocs rencontrés dans `decompteNominatif` hors des huit graphies connues — voir
    /// `ingestion_anomaly.kind = 'bloc_nominatif_inconnu'`.
    pub blocs_inconnus: Vec<String>,
}

pub struct ParsedMiseAuPoint {
    pub acteur_uid: String,
    /// Une des cinq valeurs admises par la contrainte `mise_au_point_position_connue`.
    pub position_declaree: &'static str,
}

pub struct ParsedScrutin {
    pub an_uid: String,
    pub numero: i32,
    pub legislature: i16,
    pub date_scrutin: NaiveDate,
    pub chambre: Chambre,
    pub organe_uid: Option<String>,
    pub session_ref: Option<String>,
    pub seance_ref: Option<String>,
    pub type_code: String,
    pub type_libelle: Option<String>,
    pub type_majorite: Option<String>,
    pub sort_code: Option<String>,
    pub titre: String,
    pub demandeur: Option<String>,
    pub dossier_ref: Option<String>,
    pub mode_publication: String,
    pub lieu_vote: Option<String>,
    pub nombre_votants: i32,
    pub suffrages_exprimes: i32,
    pub pour: i32,
    pub contre: i32,
    pub abstentions: i32,
    pub non_votants: i32,
    pub non_votants_volontaires: i32,
    pub groupes: Vec<ParsedGroupeVentilation>,
    pub mise_au_point: Vec<ParsedMiseAuPoint>,
}

impl ParsedScrutin {
    /// Somme des `nombreMembresGroupe` : l'effectif de l'Assemblée à la date du scrutin (D1.6).
    pub fn effectif(&self) -> i32 {
        self.groupes.iter().map(|g| g.nombre_membres).sum()
    }

    /// Nombre total de votants nominatifs, tous groupes confondus — sert au contrôle de
    /// cohérence `total nominatif = nombreVotants + nonVotants`.
    pub fn total_nominatif(&self) -> i32 {
        self.groupes.iter().map(|g| g.votes.len() as i32).sum()
    }
}

fn parse_int(s: Option<&str>) -> Option<i32> {
    s.and_then(|s| s.parse().ok())
}

/// Un bloc de `decompteNominatif` ou `miseAuPoint` est soit une liste nue (les placeholders
/// `null` d'un bloc vide), soit un objet `{"votant": ...}` normalisé par `collection`.
fn votant_entries(block: &Value) -> CollectionIter<'_> {
    match block {
        Value::Array(_) => collection(block),
        _ => collection(field(block, "votant")),
    }
}

fn parse_vote(v: &Value, position: VotePosition) -> Option<ParsedVote> {
    let acteur_uid = scalaire(field(v, "acteurRef"))?.to_string();
    Some(ParsedVote {
        acteur_uid,
        mandat_uid: scalaire(field(v, "mandatRef")).map(String::from),
        position,
        cause_non_vote: scalaire(field(v, "causePositionVote")).map(String::from),
        par_delegation: booleen(field(v, "parDelegation")),
    })
}

fn parse_groupe(g: &Value) -> ParsedGroupeVentilation {
    let vote = field(g, "vote");
    let decompte_voix = field(vote, "decompteVoix");
    let decompte_nominatif = field(vote, "decompteNominatif");

    let mut votes = Vec::new();
    let mut blocs_inconnus = Vec::new();

    if let Value::Object(blocs) = decompte_nominatif {
        for (key, value) in blocs {
            let position = match key.as_str() {
                "pours" | "pour" => VotePosition::Pour,
                "contres" | "contre" => VotePosition::Contre,
                "abstentions" | "abstention" => VotePosition::Abstention,
                "nonVotants" | "nonVotant" => VotePosition::NonVotant,
                _ => {
                    blocs_inconnus.push(key.clone());
                    continue;
                }
            };
            for entry in votant_entries(value) {
                if let Some(parsed) = parse_vote(entry, position) {
                    votes.push(parsed);
                }
            }
        }
    }

    ParsedGroupeVentilation {
        organe_uid: scalaire(field(g, "organeRef")).map(String::from),
        nombre_membres: parse_int(scalaire(field(g, "nombreMembresGroupe"))).unwrap_or(0),
        position_majoritaire: scalaire(field(vote, "positionMajoritaire")).map(String::from),
        pour: parse_int(scalaire(field(decompte_voix, "pour"))).unwrap_or(0),
        contre: parse_int(scalaire(field(decompte_voix, "contre"))).unwrap_or(0),
        abstentions: parse_int(scalaire(field(decompte_voix, "abstentions"))).unwrap_or(0),
        non_votants: parse_int(scalaire(field(decompte_voix, "nonVotants"))).unwrap_or(0),
        votes,
        blocs_inconnus,
    }
}

fn position_declaree_label(key: &str) -> Option<&'static str> {
    match key {
        "pours" => Some("pour"),
        "contres" => Some("contre"),
        "abstentions" => Some("abstention"),
        "nonVotants" => Some("non_votant"),
        "nonVotantsVolontaires" => Some("non_votant_volontaire"),
        _ => None,
    }
}

fn parse_mise_au_point(root: &Value) -> Vec<ParsedMiseAuPoint> {
    let Value::Object(blocs) = root else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (key, value) in blocs {
        let Some(position_declaree) = position_declaree_label(key) else {
            continue;
        };
        for entry in votant_entries(value) {
            if let Some(acteur_uid) = scalaire(field(entry, "acteurRef")) {
                out.push(ParsedMiseAuPoint {
                    acteur_uid: acteur_uid.to_string(),
                    position_declaree,
                });
            }
        }
    }
    out
}

/// `payload` est le document `{"scrutin": {...}}` tel que reçu.
pub fn parse_scrutin(payload: &Value) -> Option<ParsedScrutin> {
    let root = field(payload, "scrutin");
    let an_uid = scalaire(field(root, "uid"))?.to_string();
    let chambre = if an_uid.starts_with("VTCGR") {
        Chambre::Congres
    } else {
        Chambre::Assemblee
    };

    let type_vote = field(root, "typeVote");
    let sort = field(root, "sort");
    let demandeur = field(root, "demandeur");
    let objet = field(root, "objet");
    let dossier = field(objet, "dossierLegislatif");
    let synthese = field(root, "syntheseVote");
    let decompte = field(synthese, "decompte");
    let ventilation_organe = field(field(root, "ventilationVotes"), "organe");

    let groupes = collection(field(field(ventilation_organe, "groupes"), "groupe"))
        .map(parse_groupe)
        .collect();
    let mise_au_point = parse_mise_au_point(field(root, "miseAuPoint"));

    Some(ParsedScrutin {
        an_uid,
        numero: parse_int(scalaire(field(root, "numero")))?,
        legislature: scalaire(field(root, "legislature"))?.parse().ok()?,
        date_scrutin: NaiveDate::parse_from_str(scalaire(field(root, "dateScrutin"))?, "%Y-%m-%d")
            .ok()?,
        chambre,
        organe_uid: scalaire(field(root, "organeRef")).map(String::from),
        session_ref: scalaire(field(root, "sessionRef")).map(String::from),
        seance_ref: scalaire(field(root, "seanceRef")).map(String::from),
        type_code: scalaire(field(type_vote, "codeTypeVote"))?.to_string(),
        type_libelle: scalaire(field(type_vote, "libelleTypeVote")).map(String::from),
        type_majorite: scalaire(field(type_vote, "typeMajorite")).map(String::from),
        sort_code: scalaire(field(sort, "code")).map(String::from),
        titre: scalaire(field(root, "titre"))?.to_string(),
        demandeur: scalaire(field(demandeur, "texte")).map(String::from),
        dossier_ref: scalaire(field(dossier, "dossierRef")).map(String::from),
        mode_publication: scalaire(field(root, "modePublicationDesVotes"))?.to_string(),
        lieu_vote: scalaire(field(root, "lieuVote")).map(String::from),
        nombre_votants: parse_int(scalaire(field(synthese, "nombreVotants"))).unwrap_or(0),
        suffrages_exprimes: parse_int(scalaire(field(synthese, "suffragesExprimes"))).unwrap_or(0),
        pour: parse_int(scalaire(field(decompte, "pour"))).unwrap_or(0),
        contre: parse_int(scalaire(field(decompte, "contre"))).unwrap_or(0),
        abstentions: parse_int(scalaire(field(decompte, "abstentions"))).unwrap_or(0),
        non_votants: parse_int(scalaire(field(decompte, "nonVotants"))).unwrap_or(0),
        non_votants_volontaires: parse_int(scalaire(field(decompte, "nonVotantsVolontaires")))
            .unwrap_or(0),
        groupes,
        mise_au_point,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::json;

    fn base_scrutin(uid: &str, groupe_extra: Value) -> Value {
        json!({
            "scrutin": {
                "uid": uid,
                "numero": "3415",
                "organeRef": "PO717460",
                "legislature": "15",
                "sessionRef": "SCR5A2021O1",
                "seanceRef": "RUANR5L15S2021IDS23689",
                "dateScrutin": "2021-02-12",
                "typeVote": {
                    "codeTypeVote": "SPO",
                    "libelleTypeVote": "scrutin public ordinaire",
                    "typeMajorite": "majorité absolue des suffrages exprimés"
                },
                "sort": {"code": "adopté", "libelle": "l'Assemblée nationale a adopté"},
                "titre": "titre du scrutin",
                "demandeur": {"texte": "Président du groupe X"},
                "objet": {"libelle": "objet", "dossierLegislatif": null},
                "modePublicationDesVotes": "DecompteNominatif",
                "lieuVote": "Hémicycle",
                "syntheseVote": {
                    "nombreVotants": "97",
                    "suffragesExprimes": "86",
                    "decompte": {
                        "nonVotants": "2", "pour": "83", "contre": "3",
                        "abstentions": "11", "nonVotantsVolontaires": "0"
                    }
                },
                "ventilationVotes": {
                    "organe": {
                        "organeRef": "PO717460",
                        "groupes": {"groupe": groupe_extra}
                    }
                }
            }
        })
    }

    #[test]
    fn parse_scrutin_ordinaire_forme_reelle() {
        let groupe = json!({
            "organeRef": "PO730964",
            "nombreMembresGroupe": "269",
            "vote": {
                "positionMajoritaire": "pour",
                "decompteVoix": {"nonVotants": "1", "pour": "2", "contre": "0", "abstentions": "0"},
                "decompteNominatif": {
                    "nonVotants": {"votant": {"acteurRef": "PA606171", "mandatRef": "PM722632", "causePositionVote": "PAN"}},
                    "pours": {"votant": [
                        {"acteurRef": "PA1", "mandatRef": "PM1", "parDelegation": "false"},
                        {"acteurRef": "PA2", "mandatRef": "PM2", "parDelegation": "true"}
                    ]}
                }
            }
        });
        let payload = base_scrutin("VTANR5L15V3415", groupe);
        let scrutin = parse_scrutin(&payload).unwrap();

        assert_eq!(scrutin.an_uid, "VTANR5L15V3415");
        assert_eq!(scrutin.chambre, Chambre::Assemblee);
        assert_eq!(scrutin.numero, 3415);
        assert_eq!(scrutin.legislature, 15);
        assert_eq!(scrutin.effectif(), 269);
        assert_eq!(scrutin.groupes.len(), 1);
        assert_eq!(scrutin.groupes[0].votes.len(), 3);
        assert_eq!(scrutin.total_nominatif(), 3);

        let delegue = scrutin.groupes[0]
            .votes
            .iter()
            .find(|v| v.acteur_uid == "PA2")
            .unwrap();
        assert!(delegue.par_delegation);
        assert_eq!(delegue.position, VotePosition::Pour);

        let non_votant = scrutin.groupes[0]
            .votes
            .iter()
            .find(|v| v.acteur_uid == "PA606171")
            .unwrap();
        assert_eq!(non_votant.position, VotePosition::NonVotant);
        assert_eq!(non_votant.cause_non_vote.as_deref(), Some("PAN"));
    }

    #[test]
    fn parse_groupe_bloc_singulier_congres() {
        // Forme réelle mesurée sur VTCGR5L16V1 : pour/contre/abstention/nonVotant au singulier.
        let groupe = json!({
            "organeRef": "PO800538",
            "nombreMembresGroupe": "167",
            "vote": {
                "positionMajoritaire": "pour",
                "decompteVoix": {"nonVotants": "0", "pour": "1", "contre": "0", "abstentions": "0"},
                "decompteNominatif": {
                    "nonVotant": null,
                    "pour": {"votant": [{"acteurRef": "PA719118", "mandatRef": "PM796416", "parDelegation": "false"}]},
                    "contre": null,
                    "abstention": null
                }
            }
        });
        let payload = base_scrutin("VTCGR5L16V1", groupe);
        let scrutin = parse_scrutin(&payload).unwrap();
        assert_eq!(scrutin.chambre, Chambre::Congres);
        assert_eq!(scrutin.groupes[0].votes.len(), 1);
        assert!(scrutin.groupes[0].blocs_inconnus.is_empty());
    }

    #[test]
    fn parse_groupe_bloc_nominatif_inconnu_est_signale() {
        let groupe = json!({
            "organeRef": "PO1",
            "nombreMembresGroupe": "10",
            "vote": {
                "decompteVoix": {},
                "decompteNominatif": {"blocMystere": {"votant": {"acteurRef": "PA1"}}}
            }
        });
        let payload = base_scrutin("VTANR5L17V1", groupe);
        let scrutin = parse_scrutin(&payload).unwrap();
        assert_eq!(scrutin.groupes[0].votes.len(), 0);
        assert_eq!(
            scrutin.groupes[0].blocs_inconnus,
            vec!["blocMystere".to_string()]
        );
    }

    #[test]
    fn parse_groupe_fantome_po0_est_conserve_comme_organe_uid() {
        let groupe = json!({
            "organeRef": "PO0",
            "nombreMembresGroupe": "124",
            "vote": {"decompteVoix": {}, "decompteNominatif": {}}
        });
        let payload = base_scrutin("VTANR5L17V501", groupe);
        let scrutin = parse_scrutin(&payload).unwrap();
        assert_eq!(scrutin.groupes[0].organe_uid.as_deref(), Some("PO0"));
    }

    #[test]
    fn parse_scrutin_dossier_ref_reel() {
        let mut payload = base_scrutin("VTANR5L17V6722", json!([]));
        payload["scrutin"]["objet"]["dossierLegislatif"] = json!({
            "libelle": "Projet de loi...",
            "dossierRef": "DLR5L17N54083"
        });
        let scrutin = parse_scrutin(&payload).unwrap();
        assert_eq!(scrutin.dossier_ref.as_deref(), Some("DLR5L17N54083"));
    }

    #[test]
    fn parse_mise_au_point_ignore_les_placeholders_null() {
        // Forme réelle mesurée sur VTANR5L15V3415 : nonVotants/abstentions/nonVotantsVolontaires
        // sont des tableaux de `null` (bloc présent mais vide), seul `pours` porte de vraies
        // entrées.
        let mise_au_point = json!({
            "nonVotants": [null, null],
            "pours": {"votant": [
                {"acteurRef": "PA719930", "mandatRef": "PM722804"},
                {"acteurRef": "PA774958", "mandatRef": "PM774981"}
            ]},
            "abstentions": [null, null],
            "nonVotantsVolontaires": [null, null],
            "contres": null
        });
        let mut payload = base_scrutin("VTANR5L15V3415", json!([]));
        payload["scrutin"]["miseAuPoint"] = mise_au_point;
        let scrutin = parse_scrutin(&payload).unwrap();

        assert_eq!(scrutin.mise_au_point.len(), 2);
        assert!(
            scrutin
                .mise_au_point
                .iter()
                .all(|m| m.position_declaree == "pour")
        );
    }

    #[test]
    fn parse_scrutin_sans_mise_au_point_ne_plante_pas() {
        let payload = base_scrutin("VTANR5L17V1", json!([]));
        let scrutin = parse_scrutin(&payload).unwrap();
        assert!(scrutin.mise_au_point.is_empty());
    }
}
