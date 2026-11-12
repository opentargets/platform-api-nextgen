const { writeFileSync } = require('node:fs');
const feedback = require('./graphql-schema-feedback.cjs');

// Escape schema descriptions before embedding Inspector messages in Markdown.
const escape = (text) => text.replace(/&/g, '&amp;').replace(/</g, '&lt;')
  .replace(/>/g, '&gt;').replace(/([\\`*_{}\[\]()#+.!|~-])/g, '\\$1')
  .replace(/\r?\n/g, ' ');

module.exports = (changes) => {
  feedback(changes);
  const sections = [
    ['Breaking changes', changes.breakingChanges],
    ['Potentially dangerous changes', changes.dangerousChanges],
    ['Non-breaking changes', changes.nonBreakingChanges],
  ];
  const lines = ['# GraphQL schema changes', '',
    process.env.SCHEMA_BASE_DESCRIPTION, '',
    `Compared commit: \`${process.env.SCHEMA_HEAD_SHA}\``, ''];
  for (const [title, entries] of sections) {
    if (!entries.length) continue;
    lines.push(`## ${title}`, '');
    for (const entry of entries) lines.push(`- ${escape(entry.message)}`);
    lines.push('');
  }
  writeFileSync(process.env.SCHEMA_NOTES_PATH, lines.join('\n'));
};
