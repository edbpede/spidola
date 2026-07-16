// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import dev.spidola.tv.core.designsystem.SpidolaPalette
import dev.spidola.tv.core.designsystem.SpidolaRow
import dev.spidola.tv.core.designsystem.SpidolaSpacing
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

private const val LICENSE_REPORT_ASSET = "licenses/android-dependencies.json"
private const val NATIVE_LICENSE_ASSET = "licenses/native-media-lgpl-3.0.txt"
private val licenseReportJson = Json { ignoreUnknownKeys = true }

@Serializable
internal data class NoticeLicense(
    val identifier: String,
    val name: String,
    val url: String? = null,
)

@Serializable
internal data class NoticeArtifact(
    val groupId: String,
    val artifactId: String,
    val version: String,
    val spdxLicenses: List<NoticeLicense>,
) {
    val coordinate: String = "$groupId:$artifactId:$version"
    val licenseNotices: String =
        spdxLicenses.joinToString(separator = "\n") { license ->
            license.url?.let { "${license.name} — $it" } ?: license.name
        }
}

private sealed interface NoticesState {
    data object Loading : NoticesState

    data class Ready(
        val artifacts: List<NoticeArtifact>,
        val nativeLicense: String,
    ) : NoticesState

    data object Failed : NoticesState
}

internal fun parseLicenseeReport(contents: String): List<NoticeArtifact> =
    licenseReportJson
        .decodeFromString<List<NoticeArtifact>>(contents)
        .sortedBy(NoticeArtifact::coordinate)

/** Renders Licensee's packaged build report, keeping the displayed notices tied to the APK graph. */
@Composable
fun AboutScreen(
    onGoBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    val state by
        produceState<NoticesState>(initialValue = NoticesState.Loading, context) {
            value =
                withContext(Dispatchers.IO) {
                    runCatching {
                        val artifacts =
                            context.assets.open(LICENSE_REPORT_ASSET).bufferedReader().use { reader ->
                                parseLicenseeReport(reader.readText())
                            }
                        val nativeLicense =
                            context.assets.open(NATIVE_LICENSE_ASSET).bufferedReader().use { reader ->
                                reader.readText()
                            }
                        NoticesState.Ready(artifacts, nativeLicense)
                    }.fold(
                        onSuccess = { it },
                        onFailure = { NoticesState.Failed },
                    )
                }
        }

    LazyColumn(
        modifier = modifier.fillMaxSize().background(SpidolaPalette.Studio),
        contentPadding =
            PaddingValues(
                horizontal = SpidolaSpacing.safeHorizontal,
                vertical = SpidolaSpacing.safeVertical,
            ),
        verticalArrangement = Arrangement.spacedBy(SpidolaSpacing.s),
    ) {
        item(key = "title") {
            Text(
                text = stringResource(R.string.about_title),
                style = MaterialTheme.typography.displayLarge,
                color = SpidolaPalette.BroadcastWhite,
            )
        }
        item(key = "version") {
            Text(
                text = stringResource(R.string.about_version, BuildConfig.VERSION_NAME),
                style = MaterialTheme.typography.titleLarge,
                color = SpidolaPalette.BroadcastWhite,
            )
        }
        item(key = "back") {
            SpidolaRow(title = stringResource(R.string.about_go_back), onClick = onGoBack)
        }
        when (val current = state) {
            NoticesState.Loading -> item(key = "loading") { NoticeStatus(R.string.about_notices_loading) }
            NoticesState.Failed -> item(key = "failed") { NoticeStatus(R.string.about_notices_unavailable) }
            is NoticesState.Ready -> {
                item(key = "notices-title") {
                    Text(
                        text = stringResource(R.string.about_notices, current.artifacts.size),
                        style = MaterialTheme.typography.headlineMedium,
                        color = SpidolaPalette.BroadcastWhite,
                    )
                }
                item(key = "native-media-title") {
                    Text(
                        text = stringResource(R.string.about_native_media_notice),
                        style = MaterialTheme.typography.headlineMedium,
                        color = SpidolaPalette.BroadcastWhite,
                    )
                }
                item(key = "native-media-license") {
                    Text(
                        text = current.nativeLicense,
                        style = MaterialTheme.typography.bodyMedium,
                        color = SpidolaPalette.Static,
                    )
                }
                items(current.artifacts, key = NoticeArtifact::coordinate) { artifact ->
                    Text(
                        text =
                            stringResource(
                                R.string.about_dependency_notice,
                                artifact.coordinate,
                                artifact.licenseNotices,
                            ),
                        style = MaterialTheme.typography.bodyLarge,
                        color = SpidolaPalette.BroadcastWhite,
                    )
                }
            }
        }
    }
}

@Composable
private fun NoticeStatus(text: Int) {
    Text(
        text = stringResource(text),
        style = MaterialTheme.typography.bodyLarge,
        color = SpidolaPalette.BroadcastWhite,
    )
}
