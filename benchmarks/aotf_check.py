#!/usr/bin/env python3
"""Diff AOTF association scores: new API vs old OT API.

Same logical query is built in both dialects, fetched, normalized, compared
for an exact match. Only overridden datasource policies are sent; with none,
the datasource arg is omitted and each API uses its own defaults.
"""
import argparse
import json
import os
import sys
import time
import urllib.error
import urllib.request

NEW = os.environ.get("NEW_API", "http://localhost:8080/latest/graphql")
OLD = os.environ.get("OLD_API", "https://api.platform.opentargets.org/api/v4/graphql")
PAGE_SIZE = 25

# facet ids are entity-specific — a target->disease facet id is meaningless on
# disease->target. Fill each list with ids from that direction (empty = skip facet
# scenarios for that direction).
FACET_DISEASE = [  # facets on target.associatedDiseases (from your example)
    "P4DS854BUCcyDrzNLXrh", "X4HS854BUCcyDrzNMEu_", "U4DS854BUCcyDrzNLXrh",
    "noDS854BUCcyDrzNLWTa", "hIDS854BUCcyDrzNL8-3", "hoDS854BUCcyDrzNL8-3",
]
FACET_TARGET = [   # facets on disease.associatedTargets
]

# a policy override is (id, propagate, required, weight)
ALT = [("europepmc", True, False, 1.0), ("impc", True, True, 0.2)]

# label, facet, bs, bf, indirect, meas, sort, overrides
SCENARIOS = [
    dict(label="base",                     facet=True,  bs=False, bf=False, indirect=False, meas=False, sort="score",               overrides=[]),
    dict(label="no_facet",                 facet=False, bs=False, bf=False, indirect=False, meas=False, sort="score",               overrides=[]),
    dict(label="indirect",                 facet=True,  bs=False, bf=False, indirect=True,  meas=False, sort="score",               overrides=[]),
    dict(label="alt_policy",               facet=True,  bs=False, bf=False, indirect=False, meas=False, sort="score",               overrides=ALT),
    dict(label="measurements",             facet=True,  bs=False, bf=False, indirect=False, meas=True,  sort="score",               overrides=[]),
    dict(label="sort_clinical_precedence", facet=True,  bs=False, bf=False, indirect=False, meas=False, sort="clinical_precedence", overrides=[]),
    dict(label="sort_europepmc",           facet=True,  bs=False, bf=False, indirect=False, meas=False, sort="europepmc",           overrides=[]),
    dict(label="with_bs",                  facet=True,  bs=True,  bf=False, indirect=False, meas=False, sort="score",               overrides=[]),
    dict(label="with_bfilter",             facet=True,  bs=False, bf=True,  indirect=False, meas=False, sort="score",               overrides=[]),
    dict(label="bs_bf_facet",              facet=True,  bs=True,  bf=True,  indirect=False, meas=False, sort="score",               overrides=[]),
]


def b(x):
    return "true" if x else "false"


def g(w):  # 1.0 -> "1", 0.2 -> "0.2"
    return "%g" % w


def qlist(ids):
    return ",".join('"%s"' % x for x in ids)


def ds_arg(overrides, flavor):
    if not overrides:
        return ""
    if flavor == "new":
        one = lambda o: "{id: %s policy: {propagate: %s, required: %s, weight: %s}}" % (
            o[0].upper(), b(o[1]), b(o[2]), g(o[3]))
        key = "datasourcePolicyOverrides"
    else:
        one = lambda o: '{ id: "%s", propagate: %s, required: %s, weight: %s }' % (
            o[0], b(o[1]), b(o[2]), g(o[3]))
        key = "datasources"
    return "%s: [\n        %s\n      ]" % (key, "\n        ".join(one(o) for o in overrides))


def build(flavor, ctx, sc, bs_ids, bfilter):
    if flavor == "new":
        idir = "indirect: %s" % b(sc["indirect"])
        sortarg = 'sort: { key: "%s", direction: DESCENDING }' % sc["sort"]
        total, items = "total", "items"
    else:
        idir = "enableIndirect: %s" % b(sc["indirect"])
        sortarg = 'orderByScore: "%s"' % sc["sort"]
        total, items = "count", "rows"

    args = [
        "Bs: [%s]" % (qlist(bs_ids) if sc["bs"] else ""),
        'BFilter: "%s"' % (bfilter if sc["bf"] else ""),
        "facetFilters: [%s]" % (qlist(ctx["facets"]) if sc["facet"] else ""),
        idir,
    ]
    if ctx.get("meas_arg"):  # includeMeasurements only exists on associatedDiseases
        args.append("includeMeasurements: %s" % b(sc["meas"]))
    args += [
        ds_arg(sc["overrides"], flavor),
        sortarg,
        "page: { index: 0, size: %d }" % PAGE_SIZE,
    ]
    body = "\n      ".join(a for a in args if a)
    return (
        'query {\n'
        '  %(root)s(%(idarg)s: "%(idval)s") {\n'
        '    id\n'
        '    %(field)s(\n'
        '      %(body)s\n'
        '    ) {\n'
        '      %(total)s\n'
        '      %(items)s { novelty score datasourceScores { componentId: id score } }\n'
        '    }\n'
        '  }\n'
        '}'
    ) % dict(root=ctx["root"], idarg=ctx["idarg"], idval=ctx["idval"],
             field=ctx["field"], body=body, total=total, items=items)


def fetch(url, query):
    data = json.dumps({"query": query}).encode()
    req = urllib.request.Request(url, data=data, headers={"content-type": "application/json"})
    with urllib.request.urlopen(req, timeout=60) as r:
        return json.loads(r.read())


def norm(resp, ctx, flavor):
    if resp.get("errors") or resp.get("data") is None:
        return {"error": resp.get("errors", "no data")}
    node = (resp["data"].get(ctx["root"]) or {}).get(ctx["field"])
    if node is None:
        return {"error": "no association object"}
    tk, ik = ("total", "items") if flavor == "new" else ("count", "rows")
    return {
        "total": node[tk],
        "items": [
            {
                "novelty": it["novelty"],
                "score": it["score"],
                "ds": sorted(
                    ({"id": d["componentId"].lower(), "score": d["score"]}
                     for d in it["datasourceScores"]),
                    key=lambda x: x["id"],
                ),
            }
            for it in node[ik]
        ],
    }


def call(url, query):
    try:
        return fetch(url, query), None
    except urllib.error.HTTPError as e:
        return None, "HTTP %d\n%s" % (e.code, e.read().decode("utf-8", "replace"))
    except Exception as e:
        return None, str(e)


def run(ctx, sc, bs_ids, bfilter, verbose):
    qn = build("new", ctx, sc, bs_ids, bfilter)
    qo = build("old", ctx, sc, bs_ids, bfilter)
    rn, en = call(NEW, qn)
    ro, eo = call(OLD, qo)
    tag = "%s  %s" % (ctx["idval"], sc["label"])

    if en or eo:
        print("FAIL  " + tag)
        if en:
            print("--- NEW QUERY ---\n%s\n--- NEW ERROR ---\n%s" % (qn, en))
        if eo:
            print("--- OLD QUERY ---\n%s\n--- OLD ERROR ---\n%s" % (qo, eo))
        return False

    nn = norm(rn, ctx, "new")
    no = norm(ro, ctx, "old")
    if nn == no and "error" not in nn:
        print("PASS  " + tag)
        return True
    print("FAIL  " + tag)
    if verbose:
        print("--- NEW QUERY ---\n" + qn)
        print("--- OLD QUERY ---\n" + qo)
        print("--- NEW OUTPUT ---\n" + json.dumps(rn, indent=2))
        print("--- OLD OUTPUT ---\n" + json.dumps(ro, indent=2))
    return False


def main():
    p = argparse.ArgumentParser()
    p.add_argument("-t", "--targets", default="")
    p.add_argument("-d", "--diseases", default="")
    p.add_argument("-b", "--bs-diseases", default="", help="disease ids as Bs in target queries")
    p.add_argument("-B", "--bs-targets", default="", help="target ids as Bs in disease queries")
    p.add_argument("-f", "--bfilter", default="")
    p.add_argument("-s", "--sleep", type=float, default=0.3)
    p.add_argument("-v", "--verbose", action="store_true")
    a = p.parse_args()

    split = lambda s: [x for x in s.replace(",", " ").split() if x]
    targets, diseases = split(a.targets), split(a.diseases)
    bs_disease, bs_target = split(a.bs_diseases), split(a.bs_targets)
    if not targets and not diseases:
        targets = ["ENSG00000184937"]

    jobs = [(dict(root="target", field="associatedDiseases", idarg="ensemblId", idval=t, meas_arg=True, facets=FACET_DISEASE), bs_disease) for t in targets]
    jobs += [(dict(root="disease", field="associatedTargets", idarg="efoId", idval=d, facets=FACET_TARGET), bs_target) for d in diseases]

    npass = nfail = 0
    for ctx, bs_ids in jobs:
        for sc in SCENARIOS:
            if sc["bs"] and not bs_ids:
                continue
            if sc["bf"] and not a.bfilter:
                continue
            if sc["meas"] and not ctx.get("meas_arg"):
                continue
            if sc["facet"] and not ctx["facets"]:
                continue
            if run(ctx, sc, bs_ids, a.bfilter, a.verbose):
                npass += 1
            else:
                nfail += 1
            time.sleep(a.sleep)

    print("---- %d pass, %d fail ----" % (npass, nfail))
    sys.exit(1 if nfail else 0)


if __name__ == "__main__":
    main()
