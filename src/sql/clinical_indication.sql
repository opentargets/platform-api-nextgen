(
  SELECT
    ci.id AS id,
    ci.drugId AS drugId,
    ci.diseaseId AS diseaseId,
    ci.maxClinicalStage AS maxClinicalStage,
    ci.clinicalReportIds AS clinicalReportIds,
    nullIf(d.name, '') AS drugName,
    toUInt32(length(ci.clinicalReportIds)) AS clinicalReportCount,
    -- Ascending stage order, so `DESC` puts approved drugs first. Unknown stages get 0.
    -- Same order as the clinical precedence evidence view.
    toUInt64(
      indexOf(
        [
          'UNKNOWN',
          'WITHDRAWAL',
          'PRECLINICAL',
          'IND',
          'EARLY_PHASE_1',
          'PHASE_1',
          'PHASE_1_2',
          'PHASE_2',
          'PHASE_2_3',
          'PHASE_3',
          'PREAPPROVAL',
          'APPROVAL',
          'PHASE_4'
        ],
        ci.maxClinicalStage
      )
    ) AS maxClinicalStageRank
  FROM
    clinical_indication_disease AS ci
    -- A key lookup on the EmbeddedRocksDB drug table.
    LEFT JOIN drug AS d ON d.id = ci.drugId
)
