# Release rules. kotlinx.serialization, Ktor, OkHttp and Hilt ship consumer rules; these are explicit
# belt-and-braces keeps for the app's own @Serializable classes (SDUI blocks, API DTOs, nav routes), so a
# stricter R8 never strips the generated serializers that the JSON decoding looks up by Companion.serializer().
-keepattributes *Annotation*, InnerClasses, Signature, EnclosingMethod, RuntimeVisibleAnnotations

-if @kotlinx.serialization.Serializable class com.teachme.**
-keepclassmembers class <1> {
    static <1>$Companion Companion;
    *** Companion;
    kotlinx.serialization.KSerializer serializer(...);
}
-if @kotlinx.serialization.Serializable class com.teachme.** {
    static **$* *;
}
-keepclassmembers class <2>$<3> {
    kotlinx.serialization.KSerializer serializer(...);
}
-keepclassmembers class com.teachme.**$$serializer {
    *** INSTANCE;
    *** getDescriptor();
}

# Ktor optionally references JVM-management classes that do not exist on Android.
-dontwarn java.lang.management.**
-dontwarn org.slf4j.**
