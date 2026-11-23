"""Generate tests/live/<product>/*.graphql from the product's schema SDL.

Every ClickHouse-backed resolver, grouped by entity, with one alias per optional argument
(search, filter, sort, datasource weights). The ids match tests/live/fixtures.

    cargo run --example export-schema --features product-ppp > /tmp/ppp.graphql
    uv run --no-project --with graphql-core python scripts/generate-live-queries.py \
        /tmp/ppp.graphql tests/live/ppp
"""

import json
import sys
from pathlib import Path

from graphql import (
    GraphQLEnumType,
    GraphQLInputObjectType,
    GraphQLList,
    GraphQLNonNull,
    GraphQLScalarType,
    GraphQLUnionType,
    build_schema,
)

schema = build_schema(Path(sys.argv[1]).read_text())
out = Path(sys.argv[2])

ENTITIES = {'Target', 'Disease', 'Drug', 'Variant', 'Study', 'CredibleSet', 'ClinicalReport'}
# OpenSearch-backed: not ClickHouse.
SKIP = {('Query', 'search'), ('Query', 'facets'), ('Query', 'mapIds'), ('Meta', 'downloads')}
SKIP_ARGS = {'facetFilters', 'page'}

VARS = {
    'target': 'ENSG99999999999',
    'disease': 'EFO_9999999',
    'drug': 'CHEMBL9999999',
    'variant': '7_140000050_A_T',
    'study': 'GCST99999999',
    'credibleSet': 'live-test-credible-set',
    'clinicalReport': 'live-test-clinical-report',
}
BY_ARG = {
    'ensemblId': '$target', 'ensemblIds': '[$target]',
    'efoId': '$disease', 'efoIds': '[$disease]',
    'chemblId': '$drug', 'chemblIds': '[$drug]',
    'variantId': '$variant', 'variantIds': '[$variant]',
    'studyId': '$study', 'studyIds': '[$study]',
    'credibleSetId': '$credibleSet', 'credibleSetIds': '[$credibleSet]',
    'clinicalReportId': '$clinicalReport', 'clinicalReportsIds': '[$clinicalReport]',
    'diseaseIds': '[$disease]',
    'additionalIds': '[$disease]',
    'Bs': '[$disease]',
}


def unwrap(t):
    while isinstance(t, (GraphQLNonNull, GraphQLList)):
        t = t.of_type
    return t


def is_list(t):
    if isinstance(t, GraphQLNonNull):
        t = t.of_type
    return isinstance(t, GraphQLList)


def sample(t, name=''):
    """A literal of input type `t`: enums take every value in a list, the first otherwise."""
    lst = is_list(t)
    base = unwrap(t)
    if isinstance(base, GraphQLEnumType):
        values = list(base.values)
        return f'[{", ".join(values)}]' if lst else values[0]
    if isinstance(base, GraphQLInputObjectType):
        if lst and any(f == 'key' for f in base.fields):  # sort: every key
            key = unwrap(base.fields['key'].type)
            if isinstance(key, GraphQLEnumType):
                items = [f'{{key: {k}, direction: ASCENDING}}' for k in key.values]
                return f'[{", ".join(items)}]'
        obj = '{' + ', '.join(f'{n}: {sample(f.type, n)}' for n, f in base.fields.items()) + '}'
        return f'[{obj}]' if lst else obj
    scalar = {
        'String': '"a"', 'Int': '1', 'Float': '0.5', 'Boolean': 'true',
    }.get(base.name, '"a"')
    if name == 'key' and base.name == 'String':
        scalar = '"eva"'
    if name == 'policy':
        scalar = '{weight: 0.5, required: true, propagate: true}'
    return f'[{scalar}]' if lst else scalar


def arg_value(arg_name, arg):
    if arg_name in BY_ARG:
        return BY_ARG[arg_name]
    if arg_name == 'chromosome':
        return 'chr7'
    if arg_name == 'start':
        return '140000000'
    if arg_name == 'end':
        return '141000000'
    return sample(arg.type, arg_name)


def needs(arg):
    """Required: non-null with no default."""
    return isinstance(arg.type, GraphQLNonNull) and (arg.ast_node is None or arg.ast_node.default_value is None)


def fmt_args(args):
    return '(' + ', '.join(f'{k}: {v}' for k, v in args.items()) + ')' if args else ''


def leaves(t):
    return [n for n, f in t.fields.items()
            if isinstance(unwrap(f.type), (GraphQLScalarType, GraphQLEnumType)) and not f.args]


def first_scalar(t):
    return leaves(t)[0]


def shallow(t, indent):
    """Leaf fields, and `rows` one level down: enough to run a variant's SQL and decode its rows."""
    pad = '  ' * indent
    if isinstance(t, GraphQLUnionType):
        return [f'{pad}__typename']
    lines = [f'{pad}{n}' for n in leaves(t)]
    if 'rows' in t.fields:
        rows = unwrap(t.fields['rows'].type)
        lines += [f'{pad}rows {{'] + shallow(rows, indent + 1) + [f'{pad}}}']
    return lines or [f'{pad}__typename']


def selection(t, indent, depth, inside_entity, skip_fields=()):
    """Selection set lines for object type `t`."""
    pad = '  ' * indent
    if isinstance(t, GraphQLUnionType):
        lines = [f'{pad}__typename']
        for member in t.types:
            lines.append(f'{pad}... on {member.name} {{ {first_scalar(member)} }}')
        return lines
    if inside_entity and t.name in ENTITIES:
        return [f'{pad}{first_scalar(t)}']
    lines = []
    for name, field in t.fields.items():
        if (t.name, name) in SKIP or name in skip_fields:
            continue
        lines += field_lines(name, field, indent, depth, inside_entity or t.name in ENTITIES)
    return lines


def field_lines(name, field, indent, depth, inside_entity, base_args=None, alias=None, skip_fields=(), flat=False):
    """The field with its required arguments, then one alias per optional argument."""
    pad = '  ' * indent
    base = unwrap(field.type)
    args = dict(base_args or {})
    for an, a in field.args.items():
        if an not in args and needs(a):
            args[an] = arg_value(an, a)
    key = alias or name
    calls = [(f'{alias}: {name}' if alias else name, args, flat)]
    for an, a in field.args.items():
        if an in args or an in SKIP_ARGS:
            continue
        calls.append((f'{key}_{an}: {name}', args | {an: arg_value(an, a)}, True))
    lines = []
    for head, call_args, variant in calls:
        if isinstance(base, (GraphQLScalarType, GraphQLEnumType)):
            lines.append(f'{pad}{head}{fmt_args(call_args)}')
            continue
        if depth > 8:
            continue
        if variant:
            inner = shallow(base, indent + 1)
        else:
            inner = selection(base, indent + 1, depth + 1, inside_entity, skip_fields)
        if not inner:
            continue
        lines.append(f'{pad}{head}{fmt_args(call_args)} {{')
        lines += inner
        lines.append(f'{pad}}}')
    return lines


def root(name, fields_only=None, skip_fields=(), base_args=None, alias=None, flat=False):
    """A root field, its type's fields limited to `fields_only` or without `skip_fields`."""
    field = schema.query_type.fields[name]
    t = unwrap(field.type)
    if fields_only is not None:
        skip_fields = [f for f in t.fields if f not in fields_only]
    return field_lines(name, field, 1, 1, False, base_args, alias, skip_fields, flat)


def has(type_name, field):
    t = schema.type_map.get(type_name)
    return t is not None and field in t.fields


EVIDENCE = {'Target': ['evidences', 'evidenceViews'], 'Disease': ['evidences', 'evidenceViews'],
            'Variant': ['evidences']}
ASSOC = {'Target': ['associatedDiseases', 'associationTimeSeries'],
         'Disease': ['associatedTargets', 'associationTimeSeries']}
CLINICAL = {'Target': ['drugAndClinicalCandidates'], 'Disease': ['drugAndClinicalCandidates']}


def owned(type_name, *groups):
    return [f for g in groups for f in g.get(type_name, []) if has(type_name, f)]


def keep(type_name, fields):
    return ['id', 'variantId', *fields] if fields else None


groups = {
    'evidence': [
        root('target', fields_only=keep('Target', owned('Target', EVIDENCE))),
        root('disease', fields_only=keep('Disease', owned('Disease', EVIDENCE))),
        root('variant', fields_only=keep('Variant', owned('Variant', EVIDENCE))),
    ],
    'associations': [
        root('associationDatasources'),
        root('target', fields_only=keep('Target', owned('Target', ASSOC))),
        root('disease', fields_only=keep('Disease', owned('Disease', ASSOC))),
    ],
    'target': [
        root('target', skip_fields=owned('Target', EVIDENCE, ASSOC, CLINICAL)),
        root('targets', flat=True),
        root('region'),
    ],
    'clinical': [
        root('clinicalReport'),
        root('clinicalReports', flat=True),
        root('drug'),
        root('drugs', flat=True),
        root('target', fields_only=keep('Target', owned('Target', CLINICAL))),
        root('disease', fields_only=keep('Disease', owned('Disease', CLINICAL))),
    ],
    'others': [
        root('meta'),
        root('interactionResources'),
        root('disease', skip_fields=owned('Disease', EVIDENCE, ASSOC, CLINICAL)),
        root('diseases', flat=True),
        root('study'),
        root('studies', base_args={'studyIds': '[$study]'}, flat=True),
        root('studies', base_args={'diseaseIds': '[$disease]', 'enableIndirect': 'true'}, alias='studiesByDisease', flat=True),
        root('variant', skip_fields=owned('Variant', EVIDENCE)),
        root('variants', flat=True),
        root('credibleSet'),
        root('credibleSets', flat=True),
    ],
}

out.mkdir(parents=True, exist_ok=True)
for group, roots in groups.items():
    body = [line for lines in roots for line in lines]
    used = [v for v in VARS if any(f'${v}' in line for line in body)]
    decl = ', '.join(f'${v}: String!' for v in used)
    text = f'# variables: {json.dumps({v: VARS[v] for v in used})}\n'
    text += f'query Live{group.capitalize()}{f"({decl})" if decl else ""} {{\n' + '\n'.join(body) + '\n}\n'
    (out / f'{group}.graphql').write_text(text)
    print(group, len(body), 'lines')
