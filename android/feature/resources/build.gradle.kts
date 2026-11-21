plugins {
    id("teachme.android.feature")
}

android {
    namespace = "com.teachme.feature.resources"
}

dependencies {
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.browser)
}
