// Inspector prints the full diff before calling this handler. Keep schema changes
// advisory without swallowing schema-loading, parsing, or other execution errors.
module.exports = ({ breakingChanges, dangerousChanges, nonBreakingChanges }) => {
  console.log(
    `Schema review: ${breakingChanges.length} breaking, ` +
      `${dangerousChanges.length} dangerous, ${nonBreakingChanges.length} non-breaking change(s).`,
  );
  console.log('Double-check that these schema changes are intentional before committing.');
  if (breakingChanges.length) {
    console.warn('Breaking changes may require updates to API clients.');
  }
};
