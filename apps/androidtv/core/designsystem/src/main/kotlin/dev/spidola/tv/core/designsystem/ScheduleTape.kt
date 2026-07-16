// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.core.designsystem

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text

/** One fixed-height now/next schedule tape, aligned like a television continuity card. */
@Composable
fun ScheduleTape(
    currentLabel: String,
    nextLabel: String,
    currentTime: String?,
    currentTitle: String?,
    nextTime: String?,
    nextTitle: String?,
    unavailable: String,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier =
            modifier
                .fillMaxWidth()
                .heightIn(min = 104.dp)
                .background(SpidolaPalette.Set)
                .padding(SpidolaSpacing.m),
        verticalArrangement = Arrangement.spacedBy(SpidolaSpacing.s),
    ) {
        if (currentTitle == null && nextTitle == null) {
            Text(unavailable, style = MaterialTheme.typography.bodyLarge, color = SpidolaPalette.Static)
        } else {
            ScheduleLine(currentLabel, currentTime.orEmpty(), currentTitle.orEmpty(), current = true)
            ScheduleLine(nextLabel, nextTime.orEmpty(), nextTitle.orEmpty(), current = false)
        }
    }
}

@Composable
private fun ScheduleLine(
    label: String,
    time: String,
    title: String,
    current: Boolean,
) {
    Row(horizontalArrangement = Arrangement.spacedBy(SpidolaSpacing.m)) {
        Text(
            text = label,
            style = MaterialTheme.typography.labelMedium,
            color = if (current) SpidolaPalette.TestCardAmber else SpidolaPalette.Static,
            modifier = Modifier.width(76.dp),
        )
        Text(
            text = time,
            style = MaterialTheme.typography.labelMedium,
            color = SpidolaPalette.Static,
            modifier = Modifier.width(72.dp),
        )
        Text(
            text = title,
            style = MaterialTheme.typography.bodyLarge,
            color = SpidolaPalette.BroadcastWhite,
            maxLines = 1,
        )
    }
}
