(
  SELECT
    e.id AS id,
    e.score AS score,
    e.diseaseId AS diseaseId,
    dictGet('disease_names', 'name', e.diseaseId) AS diseaseName,
    e.diseaseFromSource AS diseaseFromSource,
    e.targetId AS targetId,
    dictGet('target_symbols', 'approvedSymbol', e.targetId) AS targetApprovedSymbol,
    e.targetInModel AS targetInModel,
    e.directionOnTarget AS directionOnTarget,
    e.directionOnTrait AS directionOnTrait,
    e.biologicalModelId AS biologicalModelId,
    e.biologicalModelGeneticBackground AS biologicalModelGeneticBackground,
    e.biologicalModelAllelicComposition AS biologicalModelAllelicComposition,
    e.diseaseModelAssociatedModelPhenotypes AS diseaseModelAssociatedModelPhenotypes,
    e.diseaseModelAssociatedHumanPhenotypes AS diseaseModelAssociatedHumanPhenotypes
  FROM evidence AS e
)
