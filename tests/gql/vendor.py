#!/usr/bin/env python3
"""Vendors the GraphQL documents the frontend sends to this API.

The documents are copied from `platform-webapp` into `tests/gql`, with the
fields whose resolvers read a table the integration tests do not seed removed.
The removal is recorded at the top of each file, so that it is obvious which
fields are missing and why.

The manifest of the vendored documents is written into
`tests/common/frontend_queries.rs`, between the markers below, so that the
table of the test and the documents always agree.

Usage:

    python3 tests/gql/vendor.py                    # re-vendor the documents
    python3 tests/gql/vendor.py --webapp PATH      # the frontend to read from
"""

import argparse
import glob
import json
import os
import re
import shutil
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
GQL_DIR = HERE
MANIFEST_RS = os.path.join(HERE, "..", "common", "frontend_queries.rs")
MANIFEST_JSON = os.path.join(HERE, "manifest.json")

# The documents the frontend has migrated to this API. This is the
# `MIGRATED_DOCUMENTS` list of `packages/ot-constants/codegen.ts`, plus the
# documents of the study page, which the API serves as well.
MIGRATED = [
    "apps/platform/src/pages/DiseasePage/**/*.gql",
    "apps/platform/src/pages/DrugPage/**/*.gql",
    "apps/platform/src/pages/TargetPage/**/*.gql",
    "apps/platform/src/components/AssociationsToolkit/components/Table/RowInteractors/InteractorsQuery.gql",
    "apps/platform/src/components/GeneEnrichmentAnalysis/api/*.gql",
    "packages/sections/src/disease/{Bibliography,Drugs,GWASStudies,Ontology,Phenotypes}/*.gql",
    "packages/sections/src/drug/**/*.gql",
    "packages/sections/src/target/{BaselineExpression,Bibliography,CancerHallmarks,ChemicalProbes,ComparativeGenomics,DepMap,Drugs,GeneOntology,GeneticConstraint}/*.gql",
    "packages/sections/src/target/{MolecularInteractions,MolecularStructure,MousePhenotypes,Pathways,Pharmacogenomics,QTLCredibleSets,Safety,SubcellularLocation,Tractability}/*.gql",
    "packages/ui/src/components/ClinicalReports/*.gql",
]

# The study page and the study tooltip are not in the codegen list, but the API
# serves the documents, so the tests cover them too.
EXTRA = [
    "apps/platform/src/pages/StudyPage/StudyPage.gql",
    "apps/platform/src/pages/StudyPage/StudyProfileHeader.gql",
    "packages/ui/src/components/OtAsyncTooltip/queries/StudyTooltipQuery.gql",
    "packages/sections/src/variant/VariantEffect/VariantEffectSummaryFragment.gql",
    "packages/sections/src/variant/VariantEffect/VariantEffectQuery.gql",
    "packages/sections/src/variant/VariantGenomicRegion/VariantGenomicRegionSummary.gql",
    "packages/sections/src/variant/VariantGenomicRegion/VariantGenomicRegionQuery.gql",
    "packages/sections/src/variant/MolecularStructure/MolecularStructureQuery.gql",
    "packages/sections/src/variant/Pharmacogenomics/PharmacogenomicsQuery.gql",
    "packages/sections/src/variant/EnhancerToGenePredictions/EnhancerToGenePredictionsQuery.gql",
    "packages/ui/src/components/OtAsyncTooltip/queries/VariantTooltipQuery.gql",
]

# The root query of a document, and the entity it returns.
ROOTS = {
    "target": ("target", "Target"),
    "drug": ("drug", "Drug"),
    "disease": ("disease", "Disease"),
    "study": ("study", "Study"),
    "clinicalreport": ("clinicalReport", "ClinicalReport"),
    "variant": ("variant", "Variant"),
}

# The root queries that fetch a list of entities.
LIST_ROOTS = {
    "targets": "target",
    "drugs": "drug",
    "diseases": "disease",
    "studies": "study",
    "clinicalReports": "clinicalReport",
}

# The argument the root query takes the identifier in. A fragment declares no
# variable of its own, so the wrapper around it names the variable after the
# argument of the root query.
ARGUMENT = {
    "Target": "ensemblId",
    "Drug": "chemblId",
    "Disease": "efoId",
    "Study": "studyId",
    "ClinicalReport": "clinicalReportId",
    "Variant": "variantId",
}

# The fields whose resolvers read a table the tests do not seed. They are
# removed from the documents.
BLOCKED = {
    "target": {
        "associatedDiseases", "associationTimeSeries", "baselineExpression",
        "credibleSets", "credibleSetsView", "depMapEssentiality",
        "drugAndClinicalCandidates", "evidences", "interactions", "isEssential",
        "literatureOcurrences", "mousePhenotypes", "pharmacogenomics",
        "prioritisation", "similarEntities",
    },
    "drug": {
        "adverseEvents", "drugWarnings", "indications", "literatureOcurrences",
        "mechanismsOfAction", "similarEntities",
    },
    "disease": {
        "associatedTargets", "associationTimeSeries",
        "drugAndClinicalCandidates", "evidences", "literatureOcurrences",
        "otarProjects", "phenotypes", "similarEntities", "studiesView",
    },
    "study": {
        "backgroundTraits", "biosample", "credibleSets", "credibleSetsView",
        "publicationAuthorDate",
    },
    "clinicalreport": {
        "drugs", "diseases", "sideEffects",
    },
    "variant": {
        "credibleSets", "credibleSetsView", "enhancerToGene", "evidences",
        "mostSevereConsequence", "proteinCodingCoordinates", "transcriptConsequences",
        "transcriptConsequencesView", "variantConsequences",
    },
}

# The fields inside a selection that read a table the tests do not seed. The
# selection that holds them is removed.
NESTED_BLOCKED = {
    "geneOntology": {"term"},
}

# The fields the frontend sends besides the identifier, and the value to send.
VARIABLES = {
    "size": "Variable::Number(10)",
    "index": "Variable::Number(0)",
    "pageSize": "Variable::Number(10)",
    "pageIndex": "Variable::Number(0)",
    "search": "Variable::Text",
    "sort": "Variable::EmptySort",
    "threshold": "Variable::Number(1)",
    "startYear": "Variable::Number(2000)",
    "startMonth": "Variable::Number(1)",
    "endYear": "Variable::Number(2030)",
    "endMonth": "Variable::Number(12)",
}

# The variables the document declares for the identifier.
ID_VARIABLE = {
    "target": ["ensgId", "ensemblId", "id"],
    "drug": ["chemblId", "id"],
    "disease": ["efoId", "id", "ids", "diseaseIds"],
    "study": ["studyId", "studyIds", "id", "diseaseIds"],
    "clinicalreport": ["clinicalReportId", "clinicalReportsIds", "id"],
    "variant": ["variantId", "id"],
}


def expand(webapp, patterns):
    """Returns the files the patterns match, relative to the frontend."""
    found = set()
    for pattern in patterns:
        braces = re.search(r"\{([^}]*)\}", pattern)
        if braces:
            prefix, suffix = pattern[: braces.start()], pattern[braces.end():]
            for option in braces.group(1).split(","):
                for path in glob.glob(
                    os.path.join(webapp, prefix + option + suffix), recursive=True
                ):
                    found.add(os.path.relpath(path, webapp))
        else:
            for path in glob.glob(os.path.join(webapp, pattern), recursive=True):
                found.add(os.path.relpath(path, webapp))
    return sorted(p for p in found if os.path.isfile(os.path.join(webapp, p)))


def body_from(text, start):
    """Returns the body of the selection set that starts at `start`."""
    depth = 1
    index = start
    while index < len(text) and depth > 0:
        if text[index] == "{":
            depth += 1
        elif text[index] == "}":
            depth -= 1
        index += 1
    return text[start:index - 1], index


def top_level_fields(body):
    """Returns the fields selected at the top level of a selection set.

    A field can be aliased, as in `intact: interactions(...)`. The field
    behind the alias is the one that is resolved.
    """
    fields = []
    depth = 0
    for line in body.split("\n"):
        stripped = line.strip()
        if not stripped:
            continue
        if depth == 0:
            match = re.match(r"([A-Za-z_]\w*)\s*($|:|\{|\()", stripped)
            if match:
                aliased = re.match(r"[A-Za-z_]\w*\s*:\s*([A-Za-z_]\w*)", stripped)
                fields.append(aliased.group(1) if aliased else match.group(1))
        depth += line.count("{") + line.count("(") - line.count("}") - line.count(")")
        depth = max(depth, 0)
    return fields


def remove_fields(body, blocked):
    """Removes the blocked fields from a selection set."""
    removed = []
    output = []
    depth = 0
    skipping = 0
    for line in body.split("\n"):
        stripped = line.strip()
        if depth == 0 and skipping == 0:
            match = re.match(r"([A-Za-z_]\w*)\s*($|:|\{|\()", stripped)
            name = None
            if match:
                aliased = re.match(r"[A-Za-z_]\w*\s*:\s*([A-Za-z_]\w*)", stripped)
                name = aliased.group(1) if aliased else match.group(1)
            if name and name in blocked:
                removed.append(name)
                opens = line.count("{") + line.count("(")
                closes = line.count("}") + line.count(")")
                if opens > closes:
                    skipping = 1
                continue
        if skipping > 0:
            opens = line.count("{") + line.count("(")
            closes = line.count("}") + line.count(")")
            skipping += opens - closes
            if skipping <= 0:
                skipping = 0
            continue
        output.append(line)
        depth += line.count("{") + line.count("(") - line.count("}") - line.count(")")
        depth = max(depth, 0)
    return "\n".join(output), removed


def split_parameters(parameters):
    """Splits the declaration of the variables of an operation.

    The parameters are separated by a comma or by a newline, and a default
    value can hold a comma, as in `$ids: [String!] = []`, so the brackets are
    tracked.
    """
    parts = []
    current = []
    depth = 0
    for character in parameters:
        if character in "[({":
            depth += 1
        elif character in "])}":
            depth -= 1
        if (character == "," or character == "\n") and depth == 0:
            part = "".join(current).strip()
            if part:
                parts.append(part)
            current = []
            continue
        current.append(character)
    if "".join(current).strip():
        parts.append("".join(current).strip())
    return parts


def remove_unused_variables(text):
    """Removes the variables the document declares but no longer uses.

    GraphQL rejects an operation that declares a variable it does not use, so
    the variables of the removed fields have to go as well.
    """
    declaration = re.search(r"query\s+\w*\s*\(([^)]*)\)", text, re.S)
    if not declaration:
        return text, []
    parameters = declaration.group(1)
    if "$" not in parameters:
        return text, []

    body = text[declaration.end():]
    kept = []
    unused = []
    for parameter in split_parameters(parameters):
        name = re.search(r"\$([A-Za-z_]\w*)", parameter)
        if name and f"${name.group(1)}" not in body:
            unused.append(name.group(1))
            continue
        kept.append(parameter.strip())

    if not unused:
        return text, []

    joined = "\n  " + ",\n  ".join(kept) + " " if kept else ""
    return text[: declaration.start(1)] + joined + text[declaration.end(1):], unused


def entity_of(text):
    """Returns the entity a document queries, the root query and the shape.

    The root query is the field of the response the entity is under. It is
    what tells a single entity from a list of them.
    """
    fragment = re.search(r"^fragment\s+(\w+)\s+on\s+(\w+)", text, re.M)
    if fragment:
        return fragment.group(2).lower(), None, "fragment"
    for root, entity in LIST_ROOTS.items():
        if re.search(r"\b" + root + r"\s*\(", text):
            return entity, root, "list"
    for entity, (root, _) in ROOTS.items():
        if re.search(r"\b" + root + r"\s*\(", text):
            return entity, root, "operation"
    return None, None, None


def strip(text, entity):
    """Removes the fields that read a table the tests do not seed."""
    blocked = BLOCKED.get(entity, set())
    if not blocked:
        return text, []

    removed = []
    output = text
    roots = [ROOTS[entity][0]] if entity in ROOTS else []
    roots += list(LIST_ROOTS.keys())

    for root in roots:
        pattern = re.compile(r"\b" + root + r"\s*\([^)]*\)\s*\{")
        match = pattern.search(output)
        if not match:
            continue
        body, end = body_from(output, match.end())
        blocked_here = set(top_level_fields(body)) & blocked
        if not blocked_here:
            continue
        new_body, _ = remove_fields(body, blocked_here)
        removed.extend(sorted(blocked_here))
        output = output[: match.end()] + new_body + output[end - 1:]

    # A selection can hold a field that reads another table, as
    # "geneOntology { term { ... } }". The whole selection is removed.
    for holder, blocked_nested in NESTED_BLOCKED.items():
        pattern = re.compile(r"\b" + holder + r"\s*\{", re.S)
        for match in list(pattern.finditer(output)):
            body, end = body_from(output, match.end())
            if not any(field in body for field in blocked_nested):
                continue
            removed.extend(sorted(blocked_nested))
            output = output[: match.start()] + output[end:]

    # A fragment has no root call: its selection set is the fragment body.
    type_name = re.escape(ROOTS[entity][1]) if entity in ROOTS else r"\w+"
    if not removed:
        fragment = re.search(r"(on\s+" + type_name + r"\s*\{)", output)
        if fragment:
            body, end = body_from(output, fragment.end())
            blocked_here = set(top_level_fields(body)) & blocked
            if blocked_here:
                new_body, _ = remove_fields(body, blocked_here)
                removed.extend(sorted(blocked_here))
                output = output[: fragment.end()] + new_body + output[end - 1:]

    if not removed:
        return output, []

    # A selection set cannot be empty. The identifier of the entity is always
    # there, so it is kept when nothing else is left.
    empty_root = re.compile(
        r"\b" + re.escape(roots[0] if roots else "") + r"\s*\([^)]*\)\s*\{\s*\}"
    )
    output = empty_root.sub(
        lambda m: m.group(0)[: m.group(0).index("{") + 1] + " id\n  }", output
    )
    empty_fragment = re.compile(r"(on\s+" + type_name + r"\s*\{)\s*\}")
    output = empty_fragment.sub(r"\1\n  id\n}", output)

    output, _ = remove_unused_variables(output)
    return output, sorted(set(removed))


def vendored_name(relative_path):
    """Returns the name a document is vendored under.

    The name is the path of the document without its extension, with the
    directories that are common to all the documents dropped. It has to be
    unique: two documents of the same section differ only by their file name.
    """
    relative_path = relative_path.replace(".gql", "")
    parts = relative_path.split("/")
    if parts[0] == "apps":
        # apps/platform/src/pages/<Page>[/<Sub>]/<File>
        parts = parts[4:]
    elif parts[1] == "ui":
        # packages/ui/src/components/<Component>/<File>
        parts = parts[3:]
    else:
        # packages/<package>/src/<section>/<Section>[/<Sub>]/<File>
        # The section is part of the name: the disease and the drug sections
        # hold files with the same names.
        parts = parts[3:]
    return "_".join(parts)


def id_variable(text, entity):
    """Returns the variable the document declares for the identifier."""
    variables = set(re.findall(r"\$([A-Za-z_]\w*)", text))
    for candidate in ID_VARIABLE.get(entity.lower(), []):
        if candidate in variables:
            return candidate
    return None


def extra_variables(text, identifier):
    """Returns the variables of a document besides its identifier."""
    declaration = re.search(r"query\s+\w*\s*\(([^)]*)\)", text, re.S)
    if not declaration:
        return []
    variables = []
    for name in re.findall(r"\$([A-Za-z_]\w*)", declaration.group(1)):
        if name == identifier:
            continue
        value = VARIABLES.get(name)
        if value is None:
            continue
        variables.append((name, value))
    return variables


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--webapp",
        default=os.path.join(HERE, "..", "..", "..", "platform-webapp"),
        help="the frontend to read the documents from",
    )
    parser.add_argument(
        "--expected",
        help="a file with the fields the sample data populates, per document",
    )
    arguments = parser.parse_args()
    webapp = os.path.abspath(arguments.webapp)
    if not os.path.isdir(webapp):
        sys.exit(f"the frontend is not at {webapp}")

    files = expand(webapp, MIGRATED) + [
        p for p in EXTRA if os.path.isfile(os.path.join(webapp, p))
    ]
    files = sorted(set(files))
    print(f"vendoring {len(files)} documents")

    expected = {}
    if os.path.isfile(MANIFEST_JSON):
        with open(MANIFEST_JSON) as handle:
            for name, fields in json.load(handle).get("expected", {}).items():
                expected[name] = list(fields)
    if arguments.expected and os.path.isfile(arguments.expected):
        with open(arguments.expected) as handle:
            for line in handle:
                name, _, fields = line.rstrip("\n").partition("\t")
                expected[name] = [f for f in fields.split(",") if f]

    entries = []
    for relative_path in files:
        with open(os.path.join(webapp, relative_path)) as handle:
            text = handle.read()

        entity, root, shape = entity_of(text)
        stripped, removed = strip(text, entity) if entity else (text, [])

        name = vendored_name(relative_path)
        header = ""
        if removed:
            header = (
                "# The following fields were removed, because their resolvers read a\n"
                "# table the integration tests do not seed:\n"
                + "".join(f"#   - {field}\n" for field in removed)
                + f"# The original document is {relative_path}.\n"
            )
        with open(os.path.join(GQL_DIR, f"{name}.gql"), "w") as handle:
            handle.write(header + stripped)

        if shape == "fragment":
            # A fragment declares no variable: the wrapper around it names the
            # variable after the argument of the root query it is spread into.
            variable = ARGUMENT[ROOTS[entity][1]]
            # The entity of a fragment is under the field of the root query it
            # is spread into.
            root = ROOTS[entity][0]
        else:
            variable = id_variable(text, entity)
            if variable is None:
                print(f"SKIP {name}: no identifier variable")
                continue
        entries.append(
            {
                "name": name,
                "source": relative_path,
                "entity": entity,
                "shape": shape,
                "root": root,
                "variable": variable,
                "extra": extra_variables(text, variable),
                "removed": removed,
            }
        )

    write_manifest(entries, expected)
    print(f"wrote {len(entries)} entries")


def write_manifest(entries, expected):
    """Writes the manifest into the Rust source and into a JSON file."""
    parent = {
        "target": "Some(Parent::Target)",
        "drug": "Some(Parent::Drug)",
        "disease": "Some(Parent::Disease)",
        "study": "Some(Parent::Study)",
        "clinicalreport": "Some(Parent::ClinicalReport)",
        "variant": "Some(Parent::Variant)",
    }
    root = {
        "target": "Root::Target",
        "drug": "Root::Drug",
        "disease": "Root::Disease",
        "study": "Root::Study",
        "clinicalReport": "Root::ClinicalReport",
        "variant": "Root::Variant",
        "targets": "Root::Targets",
        "drugs": "Root::Drugs",
        "diseases": "Root::Diseases",
        "studies": "Root::Studies",
        "clinicalReports": "Root::ClinicalReports",
    }
    kind = {
        "operation": "Kind::Operation",
        "fragment": "Kind::Fragment",
        "list": "Kind::RootList",
    }

    table = ["pub const QUERIES: &[FrontendQuery] = &["]
    for entry in entries:
        fields = expected.get(entry["name"], [])
        literals = ", ".join(f'"{field}"' for field in fields)
        variables = "&[" + ", ".join(
            f'("{name}", {value})' for name, value in entry["extra"]
        ) + "]"
        table.append(
            f"""    FrontendQuery {{
        name: "{entry['name']}",
        source: "{entry['source']}",
        kind: {kind[entry['shape']]},
        parent: {parent.get(entry['entity'].lower(), 'None')},
        variable: "{entry['variable']}",
        variables: {variables},
        root: {root.get(entry['root'], 'Root::Target')},
        expected: &[{literals}],
    }},"""
        )
    table.append("];")
    table = "\n".join(table)

    with open(MANIFEST_RS) as handle:
        content = handle.read()
    content = re.sub(
        r"pub const QUERIES: &\[FrontendQuery\] = &\[.*?\n\];",
        lambda _: table,
        content,
        flags=re.S,
    )
    with open(MANIFEST_RS, "w") as handle:
        handle.write(content)

    with open(MANIFEST_JSON, "w") as handle:
        json.dump(
            {
                "documents": [
                    {
                        "name": e["name"],
                        "source": e["source"],
                        "kind": e["shape"],
                        "parent": e["entity"],
                        "variable": e["variable"],
                        "removed": e["removed"],
                    }
                    for e in entries
                ],
                "expected": expected,
            },
            handle,
            indent=2,
        )
        handle.write("\n")


if __name__ == "__main__":
    main()
