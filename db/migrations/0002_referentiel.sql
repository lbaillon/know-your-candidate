-- Phase 1 — référentiel : documents source archivés bruts, personnes, organes, mandats.
--
-- Cette migration est immuable une fois mergée sur main (voir CLAUDE.md). Toute correction se fait
-- par une nouvelle migration numérotée.

CREATE TABLE source_document (
    id               bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    source           text        NOT NULL,  -- 'an_scrutin', 'an_acteur', 'an_organe', 'wikidata'
    uid              text        NOT NULL,  -- uid AN du document, ou QID
    url              text        NOT NULL,
    content_hash     text        NOT NULL,  -- sha256 du payload, calculé sur ce que nous avons reçu
    payload          jsonb       NOT NULL,
    ingestion_run_id bigint      REFERENCES ingestion_run (id),
    fetched_at       timestamptz NOT NULL DEFAULT now(),
    UNIQUE (source, uid, content_hash)
);

CREATE INDEX source_document_courant_idx ON source_document (source, uid, fetched_at DESC);

CREATE TABLE person (
    id                    bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    an_uid                text NOT NULL UNIQUE,
    wikidata_qid          text UNIQUE,
    civilite              text,
    prenom                text,
    nom                   text,
    date_naissance        date,
    ville_naissance       text,
    departement_naissance text,
    pays_naissance        text,
    date_deces            date,
    uri_hatvp             text,
    created_at            timestamptz NOT NULL DEFAULT now(),
    updated_at            timestamptz NOT NULL DEFAULT now()
);

-- Recherche par nom dès la phase 2 ; pg_trgm est déjà activé par la migration 0001.
CREATE INDEX person_nom_trgm_idx ON person USING gin (nom gin_trgm_ops);

CREATE TABLE organe (
    id             bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    an_uid         text NOT NULL UNIQUE,
    code_type      text NOT NULL,          -- 'GP', 'PARPOL', 'ASSEMBLEE'
    libelle        text NOT NULL,
    libelle_abrege text,
    libelle_abrev  text,
    legislature    smallint,
    period         daterange,
    is_non_inscrit boolean NOT NULL DEFAULT false,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX organe_type_idx ON organe (code_type, legislature);

CREATE TABLE mandat (
    id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    an_uid      text NOT NULL UNIQUE,
    person_id   bigint NOT NULL REFERENCES person (id),
    organe_id   bigint NOT NULL REFERENCES organe (id),
    type_organe text NOT NULL,
    period      daterange NOT NULL,
    qualite     text,
    legislature smallint,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT mandat_gp_sans_chevauchement
        EXCLUDE USING gist (person_id WITH =, organe_id WITH =, period WITH &&)
        WHERE (type_organe = 'GP')
);

CREATE INDEX mandat_person_idx ON mandat (person_id, type_organe);
CREATE INDEX mandat_period_idx ON mandat USING gist (period) WHERE type_organe = 'GP';

-- Pas de photo sans licence affichable : la contrainte porte la règle, pas le code applicatif.
CREATE TABLE person_photo (
    person_id    bigint PRIMARY KEY REFERENCES person (id) ON DELETE CASCADE,
    url          text NOT NULL,
    commons_file text,
    licence      text NOT NULL,
    licence_url  text,
    auteur       text,
    fetched_at   timestamptz NOT NULL DEFAULT now()
);
