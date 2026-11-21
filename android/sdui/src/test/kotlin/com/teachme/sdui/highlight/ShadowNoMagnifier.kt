package com.teachme.sdui.highlight

import android.widget.Magnifier
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements

/** The platform magnifier of a text selection crashes in Robolectric when it is dismissed (no surface): do nothing. */
@Implements(Magnifier::class)
class ShadowNoMagnifier {
    @Implementation
    fun dismiss() = Unit
}
