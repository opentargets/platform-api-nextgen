(
  SELECT
    e.id AS id,
    e.score AS score,
    e.diseaseId AS diseaseId,
    dictGet('disease_names', 'name', e.diseaseId) AS diseaseName,
    e.targetId AS targetId,
    dictGet('target_symbols', 'approvedSymbol', e.targetId) AS targetApprovedSymbol,
    e.drugId AS drugId,
    nullIf(d.name, '') AS drugName,
    arrayDistinct(arrayFlatten(arrayMap(r -> r.targets, m.rows))) AS drugTargetIds,
    arrayMap(t -> dictGet('target_symbols', 'approvedSymbol', t), drugTargetIds) AS drugTargetSymbols,
    e.directionOnTarget AS directionOnTarget,
    e.directionOnTrait AS directionOnTrait,
    e.clinicalStage AS clinicalStage,
    -- Ascending stage order, so `DESC` puts approved drugs first. Unknown and NULL stages get 0.
    toUInt64(indexOf(
      [
        'UNKNOWN', 'WITHDRAWAL', 'PRECLINICAL', 'IND', 'EARLY_PHASE_1', 'PHASE_1', 'PHASE_1_2',
        'PHASE_2', 'PHASE_2_3', 'PHASE_3', 'PREAPPROVAL', 'PHASE_4', 'APPROVAL'
      ],
      ifNull(e.clinicalStage, '')
    )) AS clinicalStageRank,
    e.clinicalReportId AS clinicalReportId,
    e.studyStartDate AS studyStartDate
  FROM evidence AS e
  LEFT JOIN drug AS d ON d.id = ifNull(e.drugId, '')
  LEFT JOIN mechanism_of_action AS m ON m.chemblId = ifNull(e.drugId, '')
)
