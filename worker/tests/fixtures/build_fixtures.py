import zipfile, json, io, copy

SRC = {
    'scrutins15.zip': 'scrutins15.zip',
    'scrutins16.zip': 'scrutins16.zip',
    'scrutins17.zip': 'scrutins17.zip',
    'amo30.zip': 'amo30.zip',
}

def load_scrutin_doc(zippath, name):
    z = zipfile.ZipFile(zippath)
    return json.loads(z.read(name))

def load_scrutin(zippath, name):
    return load_scrutin_doc(zippath, name)['scrutin']

def get_groups(scrutin):
    vv = scrutin.get('ventilationVotes', {}).get('organe', {}).get('groupes', {}).get('groupe')
    if vv is None:
        return []
    if isinstance(vv, dict):
        vv = [vv]
    return vv

def set_groups(scrutin, groups):
    scrutin['ventilationVotes']['organe']['groupes']['groupe'] = groups

def trim_block(block, keep_uids, max_keep):
    """Trim a decompteNominatif block's votant list, prioritizing acteurs in keep_uids."""
    if block is None:
        return None
    votants = block.get('votant') if isinstance(block, dict) else None
    if votants is None:
        return None
    if isinstance(votants, dict):
        votants = [votants]
    votants = [v for v in votants if isinstance(v, dict) and v.get('acteurRef')]
    if not votants:
        return None
    priority = [v for v in votants if v.get('acteurRef') in keep_uids]
    rest = [v for v in votants if v.get('acteurRef') not in keep_uids]
    kept = (priority + rest)[:max_keep]
    if not kept:
        return None
    if len(kept) == 1:
        return {'votant': kept[0]}
    return {'votant': kept}

def trim_scrutin(scrutin, groups_to_keep, max_per_block, force_keep_uids):
    groups = get_groups(scrutin)
    kept_groups = [g for g in groups if g.get('organeRef') in groups_to_keep] if groups_to_keep else groups
    new_groups = []
    for g in kept_groups:
        g = copy.deepcopy(g)
        dn = g.get('vote', {}).get('decompteNominatif', {})
        new_dn = {}
        for key, val in dn.items():
            new_dn[key] = trim_block(val, force_keep_uids, max_per_block)
        g['vote']['decompteNominatif'] = new_dn
        new_groups.append(g)
    set_groups(scrutin, new_groups)
    return scrutin

def collect_acteurs(scrutin):
    acteurs = set()
    for g in get_groups(scrutin):
        dn = g.get('vote', {}).get('decompteNominatif', {})
        for key, val in dn.items():
            if val is None:
                continue
            votants = val.get('votant') if isinstance(val, dict) else None
            if votants is None:
                continue
            if isinstance(votants, dict):
                votants = [votants]
            for v in votants:
                if isinstance(v, dict) and v.get('acteurRef'):
                    acteurs.add(v['acteurRef'])
    map_ = scrutin.get('miseAuPoint')
    if map_:
        for key, val in map_.items():
            if not isinstance(val, dict):
                continue
            votants = val.get('votant')
            if votants is None:
                continue
            if isinstance(votants, dict):
                votants = [votants]
            for v in votants:
                if isinstance(v, dict) and v.get('acteurRef'):
                    acteurs.add(v['acteurRef'])
    return acteurs

def collect_organes(scrutin):
    organes = set()
    top = scrutin.get('organeRef')
    if top:
        organes.add(top)
    for g in get_groups(scrutin):
        ref = g.get('organeRef')
        if ref and ref != 'PO0':
            organes.add(ref)
    return organes

# --- Load real scrutin documents ------------------------------------------------------------

v3415 = load_scrutin('scrutins15.zip', 'json/VTANR5L15V3415.json')
v501 = load_scrutin('scrutins17.zip', 'json/VTANR5L17V501.json')
vtcgr = load_scrutin('scrutins16.zip', 'json/VTCGR5L16V1.json')
v1 = load_scrutin('scrutins17.zip', 'json/VTANR5L17V1.json')

# Real acteurs whose votes we want to keep on purpose (delegation, causes, or missing-referentiel).
FORCE_KEEP = {
    'v3415': set(),   # will fill after inspecting delegation/cause holders below
    'v501': set(),
    'vtcgr': {'PA720634', 'PA429842'},  # 2 acteurs absents du référentiel (spike, data-sources.md)
    'v1': set(),
}

def find_delegates_and_causes(scrutin, n=3):
    found = set()
    for g in get_groups(scrutin):
        dn = g.get('vote', {}).get('decompteNominatif', {})
        for key, val in dn.items():
            if val is None:
                continue
            votants = val.get('votant') if isinstance(val, dict) else None
            if votants is None:
                continue
            if isinstance(votants, dict):
                votants = [votants]
            for v in votants:
                if not isinstance(v, dict):
                    continue
                if v.get('parDelegation') == 'true' or v.get('causePositionVote'):
                    found.add(v['acteurRef'])
                    if len(found) >= n:
                        return found
    return found

FORCE_KEEP['v3415'] |= find_delegates_and_causes(v3415, 4)
FORCE_KEEP['v501'] |= find_delegates_and_causes(v501, 4)
FORCE_KEEP['v1'] |= find_delegates_and_causes(v1, 4)
FORCE_KEEP['vtcgr'] |= find_delegates_and_causes(vtcgr, 3)

# Real acteurs of the two mise-au-point entries in V3415 (kept regardless of trimming).
mise = v3415.get('miseAuPoint', {}).get('pours', {}).get('votant', [])
if isinstance(mise, dict):
    mise = [mise]
for m in mise:
    if isinstance(m, dict) and m.get('acteurRef'):
        FORCE_KEEP['v3415'].add(m['acteurRef'])

# --- Trim -------------------------------------------------------------------------------------

trim_scrutin(v3415, {'PO730964', 'PO730934', 'PO723569'}, max_per_block=3, force_keep_uids=FORCE_KEEP['v3415'])
trim_scrutin(v501, {'PO0'}, max_per_block=2, force_keep_uids=FORCE_KEEP['v501'])
# Keep all 12 PO0 lines (that's the point of this fixture) but only the first 4 with any voters,
# trimmed to a couple of votants each — group-level fields (nombreMembresGroupe…) stay untouched.
groups_501 = get_groups(v501)
kept_501 = []
for g in groups_501[:5]:
    g = copy.deepcopy(g)
    dn = g.get('vote', {}).get('decompteNominatif', {})
    new_dn = {k: trim_block(v, FORCE_KEEP['v501'], 2) for k, v in dn.items()}
    g['vote']['decompteNominatif'] = new_dn
    kept_501.append(g)
set_groups(v501, kept_501)

trim_scrutin(vtcgr, {'PO800538', 'PO800520', 'PO800490'}, max_per_block=3, force_keep_uids=FORCE_KEEP['vtcgr'])
trim_scrutin(v1, {'PO845401', 'PO845407', 'PO840056'}, max_per_block=3, force_keep_uids=FORCE_KEEP['v1'])

def recompute_synthese(scrutin):
    """Ré-aligne syntheseVote sur les votants réellement conservés après trim, pour que seul le
    scrutin V1 (délibérément laissé incohérent) déclenche l'anomalie compteurs_incoherents."""
    counts = {'pour': 0, 'contre': 0, 'abstention': 0, 'non_votant': 0}
    for g in get_groups(scrutin):
        dn = g.get('vote', {}).get('decompteNominatif', {})
        for key, val in dn.items():
            if val is None:
                continue
            votants = val.get('votant') if isinstance(val, dict) else None
            if votants is None:
                continue
            if isinstance(votants, dict):
                votants = [votants]
            n = len(votants)
            if key in ('pours', 'pour'):
                counts['pour'] += n
            elif key in ('contres', 'contre'):
                counts['contre'] += n
            elif key in ('abstentions', 'abstention'):
                counts['abstention'] += n
            elif key in ('nonVotants', 'nonVotant'):
                counts['non_votant'] += n
    nombre_votants = counts['pour'] + counts['contre'] + counts['abstention']
    scrutin['syntheseVote']['nombreVotants'] = str(nombre_votants)
    scrutin['syntheseVote']['suffragesExprimes'] = str(counts['pour'] + counts['contre'])
    scrutin['syntheseVote']['decompte']['pour'] = str(counts['pour'])
    scrutin['syntheseVote']['decompte']['contre'] = str(counts['contre'])
    scrutin['syntheseVote']['decompte']['abstentions'] = str(counts['abstention'])
    scrutin['syntheseVote']['decompte']['nonVotants'] = str(counts['non_votant'])

for s in (v3415, v501, vtcgr):
    recompute_synthese(s)
# v1 reste tel quel : c'est le cas compteurs_incoherents (déclaré 197+10=207, nominatif trimmé
# volontairement différent).

scrutins = {
    'VTANR5L15V3415.json': v3415,
    'VTANR5L17V501.json': v501,
    'VTCGR5L16V1.json': vtcgr,
    'VTANR5L17V1.json': v1,
}

# --- Collect referenced acteurs and organes ---------------------------------------------------

all_acteurs = set()
all_organes = set()
for s in scrutins.values():
    all_acteurs |= collect_acteurs(s)
    all_organes |= collect_organes(s)

print('distinct acteurs after trimming:', len(all_acteurs))
print('distinct organes cited by scrutins:', len(all_organes))

# --- Pick two extra acteurs for the mandate-overlap normalization cases -----------------------
# (inclusion + charnière) — found by the earlier independent Python analysis of AMO30.
# We re-derive them here from AMO30 directly to keep the fixture reproducible from source alone.

amo = zipfile.ZipFile('amo30.zip')

def load_json(path):
    return json.loads(amo.read(path))

def acteur_uid(d):
    u = d['acteur']['uid']
    return u if isinstance(u, str) else u['#text']

def organe_uid(d):
    u = d['organe']['uid']
    return u if isinstance(u, str) else u['#text']

from datetime import date

def parse_date(s):
    return date.fromisoformat(s) if s else None

inclusion_uid = None
charniere_uid = None
inclusion_extra_organes = set()
charniere_extra_organes = set()

for name in amo.namelist():
    if not name.startswith('json/acteur/'):
        continue
    d = load_json(name)
    uid = acteur_uid(d)
    mandats = d['acteur'].get('mandats', {}).get('mandat')
    if mandats is None:
        continue
    if isinstance(mandats, dict):
        mandats = [mandats]
    by_organe = {}
    for m in mandats:
        if m.get('typeOrgane') != 'GP':
            continue
        oref = m.get('organes', {}).get('organeRef')
        debut = parse_date(m.get('dateDebut'))
        fin = parse_date(m.get('dateFin'))
        if debut is None:
            continue
        by_organe.setdefault(oref, []).append((debut, fin))
    for oref, periods in by_organe.items():
        if len(periods) < 2:
            continue
        periods.sort(key=lambda t: (t[0], t[1] or date.max))
        for i in range(len(periods)):
            for j in range(i + 1, len(periods)):
                d1, f1 = periods[i]
                d2, f2 = periods[j]
                f1e = f1 or date.max
                f2e = f2 or date.max
                if d2 <= f1e and d1 <= f2e:
                    if inclusion_uid is None and ((d2 >= d1 and f2e <= f1e) or (d1 >= d2 and f1e <= f2e)) and f2e != f1e:
                        inclusion_uid = uid
                        inclusion_extra_organes.add(oref)
                    elif charniere_uid is None and (d2 == f1e or d1 == f2e):
                        charniere_uid = uid
                        charniere_extra_organes.add(oref)
    if inclusion_uid and charniere_uid:
        break

print('inclusion example:', inclusion_uid, inclusion_extra_organes)
print('charniere example:', charniere_uid, charniere_extra_organes)

extra_acteurs = {u for u in (inclusion_uid, charniere_uid) if u}
all_acteurs |= extra_acteurs
all_organes |= inclusion_extra_organes | charniere_extra_organes

# --- Also pull in organes referenced by the *kept* mandats of every included acteur, so that
# ingest_acteurs never reports "organe inconnu" for a mandate it legitimately has to place ------

acteur_docs = {}
for a in all_acteurs:
    try:
        acteur_docs[a] = load_json(f'json/acteur/{a}.json')
    except KeyError:
        pass  # PA720634 / PA429842 : absents du référentiel, exactement le cas qu'on veut couvrir

for d in acteur_docs.values():
    mandats = d['acteur'].get('mandats', {}).get('mandat')
    if mandats is None:
        continue
    if isinstance(mandats, dict):
        mandats = [mandats]
    for m in mandats:
        if m.get('typeOrgane') not in ('GP', 'PARPOL', 'ASSEMBLEE'):
            continue
        oref = m.get('organes', {}).get('organeRef')
        if oref:
            all_organes.add(oref)

print('final distinct organes:', len(all_organes))
print('final distinct acteurs (incl. 2 missing):', len(all_acteurs))

organe_docs = {}
for o in all_organes:
    try:
        organe_docs[o] = load_json(f'json/organe/{o}.json')
    except KeyError:
        print('WARNING organe not found in AMO30:', o)

# --- Write the two extract archives ------------------------------------------------------------

with zipfile.ZipFile('scrutins-extrait.zip', 'w', zipfile.ZIP_DEFLATED) as z:
    for fname, doc in scrutins.items():
        z.writestr(f'json/{fname}', json.dumps({'scrutin': doc}, ensure_ascii=False))

with zipfile.ZipFile('amo30-extrait.zip', 'w', zipfile.ZIP_DEFLATED) as z:
    for uid, doc in acteur_docs.items():
        z.writestr(f'json/acteur/{uid}.json', json.dumps(doc, ensure_ascii=False))
    for uid, doc in organe_docs.items():
        z.writestr(f'json/organe/{uid}.json', json.dumps(doc, ensure_ascii=False))

import os
print('scrutins-extrait.zip:', os.path.getsize('scrutins-extrait.zip'), 'bytes')
print('amo30-extrait.zip:', os.path.getsize('amo30-extrait.zip'), 'bytes')

with open('missing_acteurs.txt', 'w') as f:
    f.write('\n'.join(sorted(all_acteurs - set(acteur_docs.keys()))))
with open('inclusion_charniere.txt', 'w') as f:
    f.write(f'inclusion_uid={inclusion_uid}\ncharniere_uid={charniere_uid}\n')
