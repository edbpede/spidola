// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.sources

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import dev.spidola.tv.core.corekit.PairingAccess
import dev.spidola.tv.core.designsystem.SpidolaPalette
import dev.spidola.tv.core.designsystem.SpidolaRow
import dev.spidola.tv.core.designsystem.SpidolaSpacing
import uniffi.core_api.PairingSession

/**
 * The LAN pairing screen (PRD §6.1): put a source on the TV from a phone, instead of typing a URL
 * with a D-pad.
 *
 * The server runs only while this screen is on screen — that lifetime *is* the security model, so
 * it is bound to composition rather than to a button. Leaving stops the server and spends the token.
 *
 * A submission never adds anything on its own: it pre-fills the add-source form for someone at the
 * TV to confirm.
 */
@Composable
fun PairingScreen(
    access: PairingAccess,
    handoff: PairingHandoff,
    onSubmissionReady: () -> Unit,
    onGoBack: () -> Unit,
    modifier: Modifier = Modifier,
    viewModel: PairingViewModel = viewModel(factory = PairingViewModel.factory(access, handoff)),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val submitted by viewModel.submitted.collectAsStateWithLifecycle()

    // Start on entry, stop on exit. `DisposableEffect` rather than `LaunchedEffect` because the stop
    // has to be prompt: waiting for the view model's scope to be torn down would leave a listener on
    // the LAN for as long as the back stack kept this entry alive.
    DisposableEffect(Unit) {
        viewModel.start()
        onDispose { viewModel.stop() }
    }

    val goToForm by rememberUpdatedState(onSubmissionReady)
    LaunchedEffect(submitted) { if (submitted) goToForm() }

    Box(modifier = modifier.fillMaxSize().background(SpidolaPalette.Studio)) {
        when (val current = state) {
            PairingState.Starting -> Centered(stringResource(R.string.pairing_starting))
            is PairingState.Failed ->
                ActionableErrorContent(
                    error = current.error,
                    onRetry = viewModel::start,
                    onGoBack = onGoBack,
                )
            is PairingState.Ready -> Instructions(session = current.session, onGoBack = onGoBack)
        }
    }
}

/** Big enough for a phone camera to lock on from across a living room. */
private val QR_SIZE = 320.dp

@Composable
private fun Instructions(
    session: PairingSession,
    onGoBack: () -> Unit,
) {
    Column(
        modifier =
            Modifier
                .fillMaxSize()
                .padding(horizontal = SpidolaSpacing.safeHorizontal, vertical = SpidolaSpacing.safeVertical)
                .widthIn(max = 1100.dp),
        verticalArrangement = Arrangement.spacedBy(SpidolaSpacing.m),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(
            text = stringResource(R.string.pairing_title),
            style = MaterialTheme.typography.displayLarge,
            color = SpidolaPalette.BroadcastWhite,
            modifier = Modifier.semantics { heading() },
        )
        Text(
            text = stringResource(R.string.pairing_explainer),
            style = MaterialTheme.typography.bodyLarge,
            color = SpidolaPalette.Static,
            textAlign = TextAlign.Center,
        )
        // Encoded once per session rather than per recomposition: the matrix depends only on the
        // URL, and the URL only changes when the server restarts.
        val qr = remember(session.url) { qrMatrixOf(session.url) }
        if (qr != null) {
            QrCode(
                matrix = qr,
                contentDescription = stringResource(R.string.pairing_qr_description),
                modifier = Modifier.size(QR_SIZE).testTag("pairing-qr"),
            )
        }
        // The address and the token are on screen as text whether or not the code drew: a QR is a
        // shortcut, never the only way in. Both are set in the display face at its largest, because
        // a person is reading them off a couch and typing them on a phone; tabular numerals come
        // with the type scale (PRD §8.3), so the digits do not shift as they read.
        Detail(
            label = stringResource(R.string.pairing_address),
            value = session.url,
            tag = "pairing-url",
        )
        Detail(
            label = stringResource(R.string.pairing_code),
            value = session.token,
            tag = "pairing-token",
        )
        SpidolaRow(
            title = stringResource(R.string.pairing_done),
            onClick = onGoBack,
            modifier = Modifier.testTag("pairing-done"),
        )
    }
}

@Composable
private fun Detail(
    label: String,
    value: String,
    tag: String,
) {
    Column(
        modifier = Modifier.fillMaxWidth().background(SpidolaPalette.Set).padding(SpidolaSpacing.m),
        verticalArrangement = Arrangement.spacedBy(SpidolaSpacing.xs),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(text = label, style = MaterialTheme.typography.labelMedium, color = SpidolaPalette.Static)
        Text(
            text = value,
            style = MaterialTheme.typography.displayLarge,
            color = SpidolaPalette.BroadcastWhite,
            textAlign = TextAlign.Center,
            modifier = Modifier.testTag(tag),
        )
    }
}

@Composable
private fun Centered(message: String) {
    Box(modifier = Modifier.fillMaxSize().padding(SpidolaSpacing.xl), contentAlignment = Alignment.Center) {
        Text(text = message, style = MaterialTheme.typography.titleLarge, color = SpidolaPalette.BroadcastWhite)
    }
}
