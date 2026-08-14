-- Phase 1 — scrutins : scrutin, ventilation par groupe, votes nominatifs, mises au point.
--
-- Cette migration est immuable une fois mergée sur main (voir CLAUDE.md). Toute correction se fait
-- par une nouvelle migration numérotée.

CREATE TYPE vote_position AS ENUM ('pour', 'contre', 'abstention', 'non_votant');
CREATE TYPE chambre AS ENUM ('assemblee', 'congres');

CREATE TABLE scrutin (
    id                      bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    an_uid                  text NOT NULL UNIQUE,
    numero                  integer NOT NULL,
    legislature             smallint NOT NULL,
    date_scrutin            date NOT NULL,
    chambre                 chambre NOT NULL DEFAULT 'assemblee',
    organe_id               bigint REFERENCES organe (id),
    session_ref             text,
    seance_ref              text,
    type_code               text NOT NULL,
    type_libelle            text,
    type_majorite           text,
    sort_code               text,
    titre                   text NOT NULL,
    demandeur               text,
    dossier_ref             text,
    mode_publication        text NOT NULL,
    lieu_vote               text,
    nombre_votants          integer NOT NULL,
    suffrages_exprimes      integer NOT NULL,
    pour                    integer NOT NULL,
    contre                  integer NOT NULL,
    abstentions             integer NOT NULL,
    non_votants             integer NOT NULL,
    non_votants_volontaires integer NOT NULL DEFAULT 0,
    effectif                integer NOT NULL,
    participation           numeric GENERATED ALWAYS AS (
                                nombre_votants::numeric / NULLIF(effectif, 0)
                            ) STORED,
    source_document_id      bigint NOT NULL REFERENCES source_document (id),
    created_at              timestamptz NOT NULL DEFAULT now(),
    updated_at              timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX scrutin_date_idx ON scrutin (date_scrutin DESC);
CREATE INDEX scrutin_participation_idx ON scrutin (participation) WHERE chambre = 'assemblee';
CREATE INDEX scrutin_titre_fts_idx ON scrutin USING gin (to_tsvector('french', titre));

-- `rang` et non `organe_id` dans la clé : les 14 scrutins au groupe fantôme publient plusieurs
-- lignes de groupe portant toutes le même `PO0`, une clé (scrutin, organe) les écraserait.
CREATE TABLE scrutin_groupe (
    scrutin_id          bigint NOT NULL REFERENCES scrutin (id) ON DELETE CASCADE,
    rang                smallint NOT NULL,
    organe_id           bigint REFERENCES organe (id),
    nombre_membres      integer NOT NULL,
    position_majoritaire text,
    pour                integer NOT NULL,
    contre              integer NOT NULL,
    abstentions         integer NOT NULL,
    non_votants         integer NOT NULL,
    PRIMARY KEY (scrutin_id, rang)
);

CREATE TABLE vote (
    scrutin_id         bigint NOT NULL REFERENCES scrutin (id) ON DELETE CASCADE,
    person_id          bigint NOT NULL REFERENCES person (id),
    position           vote_position NOT NULL,
    cause_non_vote     text,
    groupe_organe_id   bigint REFERENCES organe (id),
    groupe_from_mandat boolean NOT NULL DEFAULT false,
    mandat_an_uid      text,
    par_delegation     boolean NOT NULL DEFAULT false,
    PRIMARY KEY (scrutin_id, person_id),
    CONSTRAINT vote_cause_reservee_aux_non_votants
        CHECK (cause_non_vote IS NULL OR position = 'non_votant')
);

CREATE INDEX vote_person_idx ON vote (person_id, scrutin_id);
CREATE INDEX vote_groupe_idx ON vote (groupe_organe_id) WHERE groupe_organe_id IS NOT NULL;

CREATE TABLE vote_mise_au_point (
    scrutin_id         bigint NOT NULL REFERENCES scrutin (id) ON DELETE CASCADE,
    person_id          bigint NOT NULL REFERENCES person (id),
    position_declaree  text NOT NULL,
    source_document_id bigint NOT NULL REFERENCES source_document (id),
    PRIMARY KEY (scrutin_id, person_id, position_declaree),
    CONSTRAINT mise_au_point_position_connue CHECK (position_declaree IN (
        'pour', 'contre', 'abstention', 'non_votant', 'non_votant_volontaire'
    ))
);
