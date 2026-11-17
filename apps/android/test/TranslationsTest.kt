package net.xfbs.specto

import java.io.File
import javax.xml.parsers.DocumentBuilderFactory
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.w3c.dom.Element

/**
 * The translations (`res/values-<language>/strings.xml`) against the English texts: no texts that
 * English doesn't have, and the same placeholders. A text that depends on a number may have other
 * forms than in English, as its language needs them.
 *
 * A translation may lack texts: Android shows the English ones in their place. New texts are
 * added to the English file only, and come back from Crowdin with the translations.
 *
 * A placeholder lost or mistyped in a translation would otherwise only show, as a crash, when that
 * text comes up in that language.
 */
class TranslationsTest {
    /** A file's texts by name, with a plural's forms as "name/one", "name/other". */
    private fun texts(file: File): Map<String, String> {
        val root = DocumentBuilderFactory.newInstance().newDocumentBuilder().parse(
            file,
        ).documentElement
        val children = root.childNodes
        val texts = mutableMapOf<String, String>()
        for (i in 0 until children.length) {
            val element = children.item(i) as? Element ?: continue
            val name = element.getAttribute("name")
            when (element.tagName) {
                "string" -> if (element.getAttribute(
                        "translatable",
                    ) != "false"
                ) {
                    texts[name] = element.textContent
                }

                "plurals" -> {
                    val items = element.getElementsByTagName("item")
                    for (j in 0 until items.length) {
                        val item = items.item(j) as Element
                        texts["$name/${item.getAttribute("quantity")}"] = item.textContent
                    }
                }
            }
        }
        return texts
    }

    /** The placeholders, such as "%1$s" and "%d", in order of appearance. */
    private fun placeholders(text: String): List<String> =
        Regex("""%(\d+\$)?[sd]""").findAll(text).map { it.value }.toList().sorted()

    private val english = texts(File("res/values/strings.xml"))

    /** The name of a text without the form of a plural: "pages_selected" for both of its forms. */
    private fun String.withoutForm() = substringBefore('/')

    private val translations: Map<String, Map<String, String>> = File("res").listFiles()!!
        .filter { it.name.startsWith("values-") && File(it, "strings.xml").isFile }
        .associate { it.name.removePrefix("values-") to texts(File(it, "strings.xml")) }

    @Test
    fun thereIsAGermanTranslation() {
        assertTrue("de" in translations)
    }

    @Test
    fun translationsHaveNoTextsOfTheirOwn() {
        val names = english.keys.map { it.withoutForm() }.toSet()
        for ((language, texts) in translations) {
            // Languages differ in the plural forms they have: Russian has four, Japanese one.
            val unknown = texts.keys.map { it.withoutForm() }.filter { it !in names }.distinct()
            assertEquals(
                "texts of $language that English doesn't have",
                emptyList<String>(),
                unknown,
            )
        }
    }

    /**
     * Whether the plural form [form] of [language] stands for a single number, such as "one" in
     * English, which may then be left out of the text: "Keeps the screen on for a minute".
     *
     * In other languages the same form also stands for 21, 31 and so on (Croatian, Russian), for
     * 0 as well as 1 (French), or for 101 (Slovenian), and the number has to be there. After the
     * plural rules of the Unicode CLDR.
     */
    private fun isSingleNumber(language: String, form: String): Boolean = when (form) {
        "zero" -> language != "lv"

        "one" -> language !in setOf(
            "be", "bs", "fr", "hi", "hr", "is", "lt", "lv", "mk", "pt", "ru", "sl", "sr", "uk",
        )

        "two" -> language != "sl"

        else -> false
    }

    @Test
    fun translationsKeepThePlaceholders() {
        val wrong = mutableListOf<String>()
        for ((language, texts) in translations) {
            for ((name, text) in texts) {
                // A plural form that English doesn't have goes by the general one.
                val original = english[name] ?: english["${name.withoutForm()}/other"] ?: continue
                val expected = placeholders(original)
                val found = placeholders(text)
                val numberLeftOut = expected - found.toSet() == listOf("%d") &&
                    isSingleNumber(language, name.substringAfter('/', ""))
                if (found != expected && !numberLeftOut) {
                    wrong += "$name in $language: $found instead of $expected"
                }
            }
        }
        assertEquals("texts with other placeholders than English", emptyList<String>(), wrong)
    }
}
