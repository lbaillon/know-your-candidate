-- Phase 1 — ingestion : colonnes manquantes d'ingestion_run, table ingestion_anomaly.
--
-- Cette migration est immuable une fois mergée sur main (voir CLAUDE.md). Toute correction se fait
-- par une nouvelle migration numérotée.

ALTER TABLE ingestion_run
    ADD COLUMN job_id       bigint REFERENCES job (id) ON DELETE SET NULL,
    ADD COLUMN params       jsonb NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN url          text,
    ADD COLUMN content_hash text,
    ADD COLUMN error        text;

CREATE TABLE ingestion_anomaly (
    id               bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    ingestion_run_id bigint NOT NULL REFERENCES ingestion_run (id) ON DELETE CASCADE,
    kind             text NOT NULL,
    subject_uid      text,
    detail           jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at       timestamptz NOT NULL DEFAULT now(),
    resolved_at      timestamptz,
    resolved_by      text
);

CREATE INDEX ingestion_anomaly_kind_idx ON ingestion_anomaly (kind, created_at DESC);
