// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.sources

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.net.Inet4Address
import java.net.NetworkInterface

/**
 * The TV's own LAN address, for the pairing server to advertise.
 *
 * The shell answers this because the core cannot: `core-pair` infers the address from the route out
 * of the host, which is right on a plain LAN and wrong behind a full-tunnel VPN or on any multi-homed
 * device. This enumerates the machine's own interfaces instead of asking the routing table.
 *
 * `NetworkInterface` rather than `WifiManager`: a TV is as likely to be on Ethernet as Wi-Fi, and
 * `WifiManager.connectionInfo.ipAddress` covers only Wi-Fi, is deprecated from API 31, and would
 * cost an `ACCESS_WIFI_STATE` permission for a strictly narrower answer. Enumeration needs no
 * permission and sees both.
 *
 * Returns `null` when there is no site-local IPv4 to be found — a TV with no LAN at all. Handing
 * that `null` to the core asks it to infer, which fails loudly, and a loud failure is the right
 * outcome: pairing cannot work without a reachable address, and saying so beats advertising one
 * that will not answer.
 */
internal suspend fun lanAddress(): String? =
    withContext(Dispatchers.IO) {
        // Blocking syscalls behind a friendly name, so keep them off the main thread.
        val candidates =
            runCatching { NetworkInterface.getNetworkInterfaces()?.toList().orEmpty() }
                .getOrDefault(emptyList())
                .filter { runCatching { it.isUp && !it.isLoopback }.getOrDefault(false) }
                .flatMap { nic -> nic.inetAddresses.toList().map { nic to it } }
                .filter { (_, address) -> address is Inet4Address && address.isSiteLocalAddress }

        // Physical interfaces first. A full-tunnel VPN's `tun0` can also carry a site-local address,
        // and advertising that one gives a URL no phone on the LAN can dial — the exact failure the
        // core's docs warn about. Preferring `eth`/`wlan` picks the interface a phone shares a
        // network with; anything else is a fallback for a device that names its NICs unusually.
        val best =
            candidates.firstOrNull { (nic, _) -> nic.name.startsWith("eth") || nic.name.startsWith("wlan") }
                ?: candidates.firstOrNull()

        best?.second?.hostAddress
    }
