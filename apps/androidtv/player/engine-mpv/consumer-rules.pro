# SPDX-FileCopyrightText: 2026 Spidola contributors
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# Consumer rules for player:engine-mpv.
#
# These are not optimisation hints — without them a minified release build compiles, links,
# installs, and then crashes the first time a viewer picks the mpv engine. The JNI shim
# resolves Kotlin by *name at runtime*, which R8 cannot see and therefore feels free to rename
# or remove. Every rule below closes one such hole.

# mpv_jni.c looks this class up with FindClass("dev/spidola/tv/player/enginempv/MpvEvent") and
# resolves its constructor against the literal descriptor (IIILjava/lang/String;Ljava/lang/String;)V
# in JNI_OnLoad. R8 renaming the class, or shrinking a constructor it believes is unused
# (nothing in Kotlin calls it — only C does), makes JNI_OnLoad fail and libmpv never load.
-keep class dev.spidola.tv.player.enginempv.MpvEvent { *; }

# The native methods are bound by their fully-qualified Java name
# (Java_dev_spidola_tv_player_enginempv_MpvClient_nativeCreate, ...). Renaming the class or any
# of these methods breaks the symbol lookup with UnsatisfiedLinkError at first use.
-keep class dev.spidola.tv.player.enginempv.MpvClient {
    native <methods>;
}

# Belt and braces for any future native method in this package.
-keepclasseswithmembernames class dev.spidola.tv.player.enginempv.** {
    native <methods>;
}
