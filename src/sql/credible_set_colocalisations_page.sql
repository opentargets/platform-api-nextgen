(
  -- Joined onto the page's rows only (Table::PAGE_JOIN): display-only columns.
  -- variants is an EmbeddedRocksDB table, so this is one key lookup per row of the page.
  SELECT
    p.*,
    nullIf(v.referenceAllele, '') AS leadReferenceAllele,
    nullIf(v.alternateAllele, '') AS leadAlternateAllele
  FROM
    paged AS p
    LEFT JOIN variants AS v ON v.variantId = p.leadVariantId
)
