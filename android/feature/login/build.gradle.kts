plugins {
    id("teachme.android.feature")
}

android {
    namespace = "com.teachme.feature.login"
}

dependencies {
    implementation(project(":core:network"))
}
