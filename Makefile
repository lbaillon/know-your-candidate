.PHONY: dev lint typecheck test migrate db-up ingest css

DATABASE_URL ?= postgresql://kyc:kyc@localhost:5432/kyc

CSS_DIR := backend/src/kyc_api/static/css
CSS_OUTPUT := backend/src/kyc_api/static/style.css
# Tri lexicographique : les préfixes numériques (00-, 10-, ..., 50-) ont tous deux chiffres, donc
# l'ordre lexicographique est l'ordre de cascade voulu (D2.6).
CSS_SOURCES := $(sort $(wildcard $(CSS_DIR)/*.css))

# `podman compose up -d` rend la main avant que Postgres accepte les connexions ; au premier
# démarrage à froid, ça fait échouer `make migrate` en « connection refused ». `--wait` aide mais
# son support varie selon les versions de podman-compose, donc on ne s'y fie pas seul : on fait
# suivre d'une boucle d'attente bornée sur `pg_isready` (voir F6, docs/plans/phase-0.1-fix.md).
#
# `-h 127.0.0.1` force une vérification TCP : pendant l'initialisation de la base, l'entrypoint de
# l'image postgres démarre un serveur temporaire avec `listen_addresses=''`, qui répond sur la
# socket Unix mais pas en TCP. Sans ce drapeau, la boucle peut réussir alors que le port publié
# refuse encore les connexions — soit la panne que cette cible est censée éliminer.
db-up:
	podman compose up -d --wait 2>/dev/null || podman compose up -d
	@echo "Attente de PostgreSQL..."
	@for i in $$(seq 1 30); do \
		podman compose exec -T postgres pg_isready -U kyc -d kyc -h 127.0.0.1 >/dev/null 2>&1 && exit 0; \
		sleep 1; \
	done; \
	echo "PostgreSQL ne répond pas après 30 s. Vérifier 'podman compose logs postgres'." >&2; \
	exit 1

dev: db-up css
	$(MAKE) migrate
	@trap 'kill 0' EXIT; \
	(cd worker && cargo run) & \
	(cd backend && uv run uvicorn kyc_api.main:app --reload --port 8000) & \
	wait

# `style.css` est GÉNÉRÉ (voir sa bannière) et commité : `make lint` le régénère puis vérifie que
# rien n'a changé, sinon la CI ne peut jamais être verte avec une feuille périmée (D2.6). Personne
# n'a besoin de se souvenir de lancer `make css` avant de committer.
css:
	@{ \
		echo '/* FICHIER GÉNÉRÉ par `make css` — ne pas éditer directement, éditer $(CSS_DIR)/*.css */'; \
		echo; \
		cat $(CSS_SOURCES); \
	} > $(CSS_OUTPUT)

lint: css
	git diff --exit-code -- $(CSS_OUTPUT)
	# `../scripts` en plus de `backend/` : les outils du cycle export/import sont du Python du
	# dépôt, ils vieillissent comme le reste. La configuration ruff vit dans backend/pyproject.toml,
	# d'où l'invocation depuis backend/.
	cd backend && uv run ruff check . ../scripts && uv run ruff format --check . ../scripts
	cd worker && cargo fmt --check && cargo clippy --all-targets -- -D warnings

typecheck:
	cd backend && uv run ty check

test: db-up
	cd backend && uv run pytest
	cd worker && SQLX_OFFLINE=true cargo test

migrate:
	sqlx migrate run --source db/migrations --database-url "$(DATABASE_URL)"

# Enchaîne l'ingestion complète, **en cinq étapes séparées par un `run-once`** — une par niveau
# de dépendance. Ce découpage n'est pas cosmétique : la file est ordonnée par priorité puis par
# date de planification, et un job repris après un échec repasse DERRIÈRE ceux enfilés après lui.
# En une seule file, un simple incident réseau sur `ingest_scrutins` suffisait donc à faire tourner
# `label_scrutins_heuristic` et `refresh_views` sur un corpus incomplet — sans aucune erreur, tous
# les jobs finissant bien en `done` (F6, docs/plans/phase-3.0-feedback.md). `run-once` ne rend la
# main qu'une fois la file vide, reprises comprises : une étape ne peut plus doubler celle dont
# elle dépend.
#
# Compte en dizaines de minutes sur les trois législatures complètes — c'est attendu, pas un hang.
# `run-once` sort en code non nul si l'un des jobs échoue (F3) : `make ingest` s'arrête avec lui,
# et s'arrête désormais à l'étape fautive plutôt qu'après avoir calculé des dérivés faux.
ingest: db-up
	$(MAKE) migrate
	@echo "==> Étape 1/5 : référentiel (person, organe, mandat)"
	cd worker && cargo run --release -- enqueue ingest_acteurs
	cd worker && cargo run --release -- run-once
	@echo "==> Étape 2/5 : scrutins des législatures 15-17 et enrichissement Wikidata"
	cd worker && cargo run --release -- enqueue ingest_scrutins '{"legislature": 15}'
	cd worker && cargo run --release -- enqueue ingest_scrutins '{"legislature": 16}'
	cd worker && cargo run --release -- enqueue ingest_scrutins '{"legislature": 17}'
	cd worker && cargo run --release -- enqueue enrich_wikidata
	cd worker && cargo run --release -- run-once
	@echo "==> Étape 3/5 : seed éditorial des candidat·es (peut créer des personnes)"
	cd worker && cargo run --release -- enqueue seed_candidates
	cd worker && cargo run --release -- run-once
	@echo "==> Étape 4/5 : slugs (toutes les personnes existent enfin) et thèmes"
	cd worker && cargo run --release -- enqueue assign_slugs
	cd worker && cargo run --release -- enqueue seed_themes
	cd worker && cargo run --release -- run-once
	@echo "==> Étape 5/5 : dérivés (estimations d'axe, scores, vues matérialisées)"
	cd worker && cargo run --release -- enqueue label_scrutins_heuristic
	cd worker && cargo run --release -- enqueue recompute_scores
	cd worker && cargo run --release -- enqueue refresh_views
	cd worker && cargo run --release -- run-once
