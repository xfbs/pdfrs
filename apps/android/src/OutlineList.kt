package net.xfbs.specto

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import uniffi.specto_ffi.OutlineEntry

/**
 * The indices of the [entries] to show, given the indices of the [expanded] ones: an entry is shown
 * unless one of its ancestors is collapsed.
 */
internal fun visibleOutlineRows(entries: List<OutlineEntry>, expanded: Set<Int>): List<Int> =
    buildList {
        var hiddenBelow = Int.MAX_VALUE
        for ((i, entry) in entries.withIndex()) {
            val depth = entry.depth.toInt()
            if (depth > hiddenBelow) continue
            hiddenBelow = Int.MAX_VALUE
            add(i)
            if (entry.hasChildren && i !in expanded) hiddenBelow = depth
        }
    }

/** Outlines up to this size start fully expanded. Longer ones show their top level. */
private const val EXPAND_ALL_UP_TO = 40

/**
 * The document's outline (table of contents) as a collapsible list.
 *
 * Tapping an entry calls [onPick] with its index into [PdfViewState.outline].
 */
@Composable
fun OutlineList(state: PdfViewState, onPick: (Int) -> Unit, modifier: Modifier = Modifier) {
    val entries = state.outline
    var expanded by remember(state) {
        mutableStateOf(
            if (entries.size <= EXPAND_ALL_UP_TO) {
                entries.indices.filter { entries[it].hasChildren }.toSet()
            } else {
                emptySet()
            },
        )
    }
    val visible = remember(entries, expanded) { visibleOutlineRows(entries, expanded) }
    // Where the reader is: the last entry at or before the current page.
    val currentPage = state.currentPage
    val here = entries.indexOfLast { e -> e.page?.let { it.toInt() <= currentPage } == true }
    val list = rememberLazyListState(
        initialFirstVisibleItemIndex = (visible.indexOf(here) - 3).coerceAtLeast(0),
    )

    LazyColumn(modifier, state = list, contentPadding = PaddingValues(vertical = 8.dp)) {
        items(visible, key = { it }) { i ->
            val entry = entries[i]
            Row(
                Modifier
                    .fillMaxWidth()
                    .clickable { onPick(i) }
                    .heightIn(min = 48.dp)
                    .padding(start = (4 + 20 * entry.depth.toInt()).dp, end = 16.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                if (entry.hasChildren) {
                    val open = i in expanded
                    IconButton(onClick = { expanded = if (open) expanded - i else expanded + i }) {
                        Icon(
                            if (open) {
                                Icons.Filled.KeyboardArrowDown
                            } else {
                                Icons.AutoMirrored.Filled.KeyboardArrowRight
                            },
                            contentDescription = stringResource(
                                if (open) R.string.collapse else R.string.expand,
                            ),
                        )
                    }
                } else {
                    Box(Modifier.size(48.dp))
                }
                val scheme = MaterialTheme.colorScheme
                val color = if (i == here) scheme.primary else scheme.onSurface
                Text(
                    entry.title,
                    Modifier.weight(1f).padding(vertical = 8.dp),
                    color = color,
                    fontWeight = if (i == here) FontWeight.SemiBold else null,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
                entry.page?.let {
                    Text(
                        state.pageLabel(it.toInt()),
                        Modifier.padding(start = 12.dp),
                        style = MaterialTheme.typography.labelLarge,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }
    }
}
