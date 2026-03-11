import { parse, type ParseError, printParseErrorCode } from 'jsonc-parser'

export function stripJsonComments(input: string): string {
  let output = ''
  let inString = false
  let escaped = false
  let inLineComment = false
  let inBlockComment = false

  for (let i = 0; i < input.length; i += 1) {
    const current = input[i]
    const next = input[i + 1]

    if (inLineComment) {
      if (current === '\n' || current === '\r') {
        inLineComment = false
        output += current
      } else {
        output += ' '
      }
      continue
    }

    if (inBlockComment) {
      if (current === '*' && next === '/') {
        output += '  '
        inBlockComment = false
        i += 1
      } else if (current === '\n' || current === '\r') {
        output += current
      } else {
        output += ' '
      }
      continue
    }

    if (inString) {
      output += current
      if (escaped) {
        escaped = false
      } else if (current === '\\') {
        escaped = true
      } else if (current === '"') {
        inString = false
      }
      continue
    }

    if (current === '"') {
      inString = true
      output += current
      continue
    }

    if (current === '/' && next === '/') {
      output += '  '
      inLineComment = true
      i += 1
      continue
    }

    if (current === '/' && next === '*') {
      output += '  '
      inBlockComment = true
      i += 1
      continue
    }

    output += current
  }

  return output
}

export function parseJsonWithComments<T = unknown>(input: string): T {
  const errors: ParseError[] = []
  const parsed = parse(input, errors, {
    disallowComments: false,
    allowTrailingComma: true,
    allowEmptyContent: false,
  })

  if (errors.length > 0) {
    const first = errors[0]
    throw new Error(`JSON 语法错误(${printParseErrorCode(first.error)})，位置 ${first.offset}`)
  }

  return parsed as T
}
