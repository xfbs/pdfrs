package net.xfbs.specto

import java.io.File
import javax.xml.parsers.DocumentBuilderFactory
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.w3c.dom.Element

/**
 * The translations (`res/values-<language>/strings.xml`) against the English texts: each has every
 * text and no others, with the same placeholders. A text that depends on a number may have other
 * forms than in English, as its language needs them.
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
    fun translationsHaveEveryTextAndNoOthers() {
        for ((language, texts) in translations) {
            // Languages differ in the plural forms they have: Russian has four, Japanese one.
            assertEquals(
                "texts of $language",
                english.keys.map { it.withoutForm() }.distinct().sorted(),
                texts.keys.map { it.withoutForm() }.distinct().sorted(),
            )
        }
    }

    @Test
    fun translationsKeepThePlaceholders() {
        for ((language, texts) in translations) {
            for ((name, text) in texts) {
                // A plural form that English doesn't have goes by the general one.
                val original = english[name] ?: english["${name.withoutForm()}/other"] ?: continue
                assertEquals("$name in $language", placeholders(original), placeholders(text))
            }
        }
    }
}
