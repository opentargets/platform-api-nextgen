(
    -- Columns listed explicitly, in the same order as `GwasCredibleSetsByVariant`:
    -- rows are decoded by position, so this keeps decoding independent of the
    -- table's own column order.
    SELECT
        variantId,
        studyLocusId,
        posteriorProbability,
        locusSize,
        pValueMantissa,
        pValueExponent,
        beta,
        finemappingMethod,
        confidence,
        leadVariantId,
        leadChromosome,
        leadPosition,
        leadReferenceAllele,
        leadAlternateAllele,
        studyId,
        traitFromSource,
        diseases,
        diseaseTherapeuticAreas,
        topL2GGeneId,
        topL2GApprovedSymbol,
        topL2GScore
    FROM variant_credible_sets
    WHERE studyType = 'gwas'
)
