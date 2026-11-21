package com.teachme.sdui.render

enum class TokenKind { Keyword, Str, Comment, Number, Function, Type, Punctuation }

data class Token(
    val start: Int,
    val end: Int,
    val kind: TokenKind,
)

private class LangSpec(
    val keywords: Set<String>,
    val lineComments: List<String> = listOf("//"),
    val blockComments: Boolean = true,
    val quotes: String = "\"",
    val macroBang: Boolean = false,
    val lifetimes: Boolean = false,
)

private fun words(text: String) = text.split(' ', '\n').filter { it.isNotEmpty() }.toSet()

private val C_LIKE_KEYWORDS =
    words(
        "if else for while do switch case break continue return goto sizeof struct union enum typedef static const void int char float double long short unsigned signed extern volatile register inline auto new delete class public private protected virtual namespace using template typename this nullptr true false",
    )

private val SPECS: Map<String, LangSpec> =
    mapOf(
        "rust" to
            LangSpec(
                words(
                    "as break const continue crate dyn else enum extern false fn for if impl in let loop match mod move mut pub ref return self Self static struct super trait true type unsafe use where while async await",
                ),
                quotes = "\"'",
                macroBang = true,
                lifetimes = true,
            ),
        "kotlin" to
            LangSpec(
                words(
                    "as break class continue do else false for fun if in interface is null object override package return super this throw true try typealias val var when while by catch finally import private public protected internal open abstract sealed data enum companion init constructor suspend inline",
                ),
                quotes = "\"'",
            ),
        "java" to
            LangSpec(
                words(
                    "abstract boolean break byte case catch char class const continue default do double else enum extends final finally float for if implements import instanceof int interface long new null package private protected public return short static super switch this throw throws true false try void volatile while var record",
                ),
                quotes = "\"'",
            ),
        "python" to
            LangSpec(
                words(
                    "and as assert async await break class continue def del elif else except False finally for from global if import in is lambda None nonlocal not or pass raise return True try while with yield",
                ),
                lineComments = listOf("#"),
                blockComments = false,
                quotes = "\"'",
            ),
        "javascript" to
            LangSpec(
                words(
                    "async await break case catch class const continue default delete do else export extends false finally for function if import in instanceof let new null of return static super switch this throw true try typeof undefined var void while yield",
                ),
                quotes = "\"'`",
            ),
        "typescript" to
            LangSpec(
                words(
                    "abstract any as async await boolean break case catch class const continue declare default delete do else enum export extends false finally for function if implements import in instanceof interface let new null number of private protected public readonly return static string super switch this throw true try type typeof undefined var void while yield",
                ),
                quotes = "\"'`",
            ),
        "c" to LangSpec(C_LIKE_KEYWORDS, quotes = "\"'"),
        "cpp" to LangSpec(C_LIKE_KEYWORDS, quotes = "\"'"),
        "go" to
            LangSpec(
                words(
                    "break case chan const continue default defer else fallthrough for func go goto if import interface map package range return select struct switch type var true false nil",
                ),
                quotes = "\"'`",
            ),
        "sql" to
            LangSpec(
                words(
                    "select from where insert into values update set delete create table drop alter add join left right inner outer on group by order having limit offset and or not null as distinct union primary key foreign references index unique default in is like between exists case when then else end SELECT FROM WHERE INSERT INTO VALUES UPDATE SET DELETE CREATE TABLE DROP ALTER ADD JOIN LEFT RIGHT INNER OUTER ON GROUP BY ORDER HAVING LIMIT OFFSET AND OR NOT NULL AS DISTINCT UNION PRIMARY KEY FOREIGN REFERENCES INDEX UNIQUE DEFAULT IN IS LIKE BETWEEN EXISTS CASE WHEN THEN ELSE END",
                ),
                lineComments = listOf("--"),
                quotes = "\"'",
            ),
        "json" to LangSpec(words("true false null"), lineComments = emptyList(), blockComments = false),
        "bash" to
            LangSpec(
                words(
                    "if then else elif fi for while do done case esac in function return export local echo cd exit set unset source alias",
                ),
                lineComments = listOf("#"),
                blockComments = false,
                quotes = "\"'",
            ),
        "toml" to LangSpec(words("true false"), lineComments = listOf("#"), blockComments = false, quotes = "\"'"),
        "yaml" to
            LangSpec(
                words("true false null yes no"),
                lineComments = listOf("#"),
                blockComments = false,
                quotes = "\"'",
            ),
    )

private val ALIASES =
    mapOf(
        "rs" to "rust",
        "kt" to "kotlin",
        "kts" to "kotlin",
        "py" to "python",
        "js" to "javascript",
        "ts" to "typescript",
        "c++" to "cpp",
        "cc" to "cpp",
        "h" to "c",
        "golang" to "go",
        "sh" to "bash",
        "shell" to "bash",
        "zsh" to "bash",
        "yml" to "yaml",
    )

/**
 * A small hand written tokenizer for the code blocks (docs/design/components/code.md): colors are a
 * redundancy, never the carrier of meaning. Unknown languages (`text`, `output`...) get no tokens. It never
 * throws and is linear in the size of the code.
 */
object SyntaxHighlighter {
    fun supports(language: String): Boolean = SPECS.containsKey(ALIASES[language.lowercase()] ?: language.lowercase())

    fun tokenize(
        code: String,
        language: String,
    ): List<Token> {
        val spec = SPECS[ALIASES[language.lowercase()] ?: language.lowercase()] ?: return emptyList()
        val out = mutableListOf<Token>()
        var i = 0
        val n = code.length
        while (i < n) {
            val c = code[i]
            when {
                c.isWhitespace() -> i++
                spec.lineComments.any { code.startsWith(it, i) } -> {
                    val end = code.indexOf('\n', i).let { if (it < 0) n else it }
                    out += Token(i, end, TokenKind.Comment)
                    i = end
                }
                spec.blockComments && code.startsWith("/*", i) -> {
                    val close = code.indexOf("*/", i + 2)
                    val end = if (close < 0) n else close + 2
                    out += Token(i, end, TokenKind.Comment)
                    i = end
                }
                spec.lifetimes && c == '\'' && isLifetime(code, i) -> {
                    var j = i + 1
                    while (j < n && (code[j].isLetterOrDigit() || code[j] == '_')) j++
                    out += Token(i, j, TokenKind.Type)
                    i = j
                }
                c in spec.quotes -> {
                    val end = stringEnd(code, i)
                    out += Token(i, end, TokenKind.Str)
                    i = end
                }
                c.isDigit() -> {
                    var j = i + 1
                    while (j < n &&
                        (
                            code[j].isLetterOrDigit() ||
                                code[j] == '_' ||
                                code[j] == '.' &&
                                j + 1 < n &&
                                code[j + 1].isDigit()
                        )
                    ) {
                        j++
                    }
                    out += Token(i, j, TokenKind.Number)
                    i = j
                }
                c.isLetter() || c == '_' -> {
                    var j = i + 1
                    while (j < n && (code[j].isLetterOrDigit() || code[j] == '_')) j++
                    val word = code.substring(i, j)
                    val next = nextSignificant(code, j)
                    val kind =
                        when {
                            word in spec.keywords -> TokenKind.Keyword
                            next == '(' -> TokenKind.Function
                            spec.macroBang && next == '!' && j < n && code[j] == '!' -> TokenKind.Function
                            word.first().isUpperCase() -> TokenKind.Type
                            else -> null
                        }
                    if (kind != null) out += Token(i, j, kind)
                    i = j
                }
                c in "{}[]();,.:<>=+-*/&|!?%^~@\\" -> {
                    out += Token(i, i + 1, TokenKind.Punctuation)
                    i++
                }
                else -> i++
            }
        }
        return out
    }

    /** First char at or after [from] that is not blank (a line break counts), or null. */
    private fun nextSignificant(
        code: String,
        from: Int,
    ): Char? {
        var k = from
        while (k < code.length) {
            val c = code[k]
            if (!c.isWhitespace() || c == '\n') return c
            k++
        }
        return null
    }

    /** Rust `'a` lifetime vs `'a'` char literal. */
    private fun isLifetime(
        code: String,
        i: Int,
    ): Boolean {
        if (i + 1 >= code.length || !(code[i + 1].isLetter() || code[i + 1] == '_')) return false
        var j = i + 1
        while (j < code.length && (code[j].isLetterOrDigit() || code[j] == '_')) j++
        return !(j < code.length && code[j] == '\'')
    }

    /** End (exclusive) of the string literal starting at [start]; strings end at the closing quote or the line end. */
    private fun stringEnd(
        code: String,
        start: Int,
    ): Int {
        val quote = code[start]
        var j = start + 1
        while (j < code.length) {
            val c = code[j]
            when {
                c == '\\' -> j += 2
                c == quote -> return j + 1
                c == '\n' && quote != '`' -> return j
                else -> j++
            }
        }
        return code.length
    }
}
