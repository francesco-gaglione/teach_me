plugins {
    id("teachme.android.feature")
}

android {
    namespace = "com.teachme.feature.mission"

    testOptions.unitTests.isIncludeAndroidResources = true
}

dependencies {
    debugImplementation(libs.compose.ui.test.manifest)
    testImplementation(libs.compose.ui.test.junit4)
    testImplementation(libs.androidx.test.core)
    testImplementation(libs.androidx.test.ext.junit)
    testImplementation(libs.robolectric)
}

// Robolectric instruments the JDK classes with ASM: it needs the JDK the project compiles for (21), not a newer one.
tasks.withType<Test>().configureEach {
    javaLauncher.set(javaToolchains.launcherFor { languageVersion.set(JavaLanguageVersion.of(21)) })
}
