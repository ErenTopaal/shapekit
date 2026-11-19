// how the landing page writes a finding's short statement.
//
// findings filed upstream as questions keep a "question:" prefix in their short statement, and
// candidate statements are written lowercase with no closing period. the page shows the question
// as a label and every statement as a sentence. a statement that opens with a stellar cli command
// keeps the command's lowercase spelling.
export function statement(title: string) {
  const q = /^question:\s*/i.exec(title)
  let text = q ? title.slice(q[0].length) : title
  if (!/^stellar [a-z]/.test(text)) text = text.charAt(0).toUpperCase() + text.slice(1)
  if (!/[.!?]$/.test(text)) text += "."
  return { question: q !== null, text }
}

// statements name sdk identifiers such as ManageSellOfferOperationBuilder.setOfferId, which are
// wider than the finding card on a phone. the text is split where a line may break inside such a
// name: after a dot or :: that joins two names, and between the words of a long camel-case
// name. the caller puts a <wbr> between the segments.
export function breakSegments(text: string): string[] {
  const marked = text.replace(/[A-Za-z0-9_.:]{16,}/g, (word) =>
    word.replace(/(\.|::)(?=[A-Za-z])/g, "$1\u0000").replace(/([a-z0-9])(?=[A-Z][a-z])/g, "$1\u0000"),
  )
  return marked.split("\u0000")
}
