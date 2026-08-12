# Phase 5 — Déploiement, observabilité, sauvegardes

**Statut : 📝 à relire** · Dépend de : phases 0 à 4 (mais à démarrer tôt, voir ci-dessous)

## Objectif

Mettre le site en ligne de façon reproductible, observable et sauvegardée, pour un coût proche de zéro.

## À lire en premier : Vercel ne convient pas à ce projet

C'était une des cibles envisagées, il faut trancher tôt car cela conditionne l'architecture.

| Besoin | Vercel | Render |
| --- | --- | --- |
| FastAPI | ✅ en fonctions serverless | ✅ service web classique |
| Worker Rust permanent | ❌ pas de processus long | ✅ *background worker* |
| Connexion `LISTEN` maintenue | ❌ incompatible avec le serverless | ✅ |
| PostgreSQL managé | ❌ base externe requise | ✅ intégré |
| Deux processus partageant une base | ❌ base externe, latence | ✅ réseau privé interne |

**Proposition : Render.com** — un service web (FastAPI), un *background worker* (Rust) et une base
PostgreSQL managée, dans le même réseau privé. C'est l'agencement qui correspond exactement à
l'architecture décrite.

Vercel reste envisageable pour une variante « site statique + base externe » où l'ingestion tournerait
ailleurs, mais cela retirerait au projet son intérêt d'actualisation continue. Fly.io est une alternative
crédible à Render si les limites de l'offre gratuite deviennent gênantes.

## À faire tôt, sans attendre la fin

Le déploiement est numéroté 5 mais trois éléments doivent exister dès la phase 0 ou 1 :

1. **CI** (phase 0) : lint, typage, tests, migrations rejouées.
2. **Sauvegarde de la base** dès qu'il y a des données catégorisées à la main : ce travail humain est
   irremplaçable, sa perte tuerait le projet.
3. **Logfire** dès la phase 1 : sans traces, déboguer une ingestion de dizaines de milliers de lignes se
   fait à l'aveugle.

## Livrables

1. `render.yaml` décrivant les trois composants et leurs variables d'environnement.
2. Déploiement automatique sur `main` après CI verte, migrations jouées au démarrage de manière sûre
   (verrou consultatif pour éviter deux exécutions simultanées).
3. Logfire branché côté FastAPI (instrumentation automatique) et côté worker via OTLP, avec un
   échantillonnage compatible avec l'offre gratuite.
4. Sauvegardes : `pg_dump` planifié vers un stockage externe, **restauration testée au moins une fois**.
5. Page de statut publique minimale : date de la dernière ingestion, nombre de scrutins, de votes, de
   catégorisations relues.
6. Documentation d'exploitation : comment lancer une ingestion, relancer un job bloqué, restaurer.

## Étapes

1. Dockerfiles (ou builds natifs Render) pour le backend et le worker.
2. `render.yaml`, variables d'environnement, secrets.
3. Stratégie de migration au démarrage, avec verrou.
4. Logfire : traces, métriques d'ingestion, alerte simple sur les échecs de job.
5. Sauvegardes et test de restauration.
6. Nom de domaine, HTTPS, `robots.txt`, page de statut.

## Décisions à trancher

| # | Question | Proposition |
| --- | --- | --- |
| D5.1 | Hébergeur | **Render** pour la correspondance avec l'architecture ; Fly.io en solution de repli |
| D5.2 | Ingestion en production ou en local ? | Si l'offre gratuite ne tient pas la charge de parsing, ingérer en local et importer un dump. À décider avec les chiffres du spike de la phase 1 |
| D5.3 | Volume de données envoyé à Logfire | Échantillonner les traces d'ingestion, garder toutes les erreurs |
| D5.4 | Sauvegarde | `pg_dump` planifié vers un stockage objet ; chiffrer si l'espace n'est pas privé |
| D5.5 | Mise en veille des offres gratuites | Un service web gratuit s'endort : première visite lente. Acceptable en v1, à revoir à la mise en avant publique |
| D5.6 | Domaine | À choisir ; éviter un nom qui suggère une prise de position |

## Fini quand

- Un push sur `main` déploie automatiquement après CI verte.
- Le worker traite un job déclenché depuis l'interface d'administration en production.
- Une trace Logfire montre une ingestion complète, étape par étape.
- Une restauration de sauvegarde a été réalisée pour de vrai, et la procédure est écrite.
- La page de statut affiche des chiffres exacts.

## Risques

- **Limites de l'offre gratuite** : mémoire et CPU serrés pour du parsing massif. Le repli « ingestion en
  local + import de dump » doit rester possible par construction.
- **Migrations au démarrage** : deux instances qui migrent en même temps. Le verrou consultatif n'est pas
  optionnel.
- **Coût de sortie** : garder les composants portables (Postgres standard, pas de service propriétaire)
  pour pouvoir changer d'hébergeur sans réécriture.
