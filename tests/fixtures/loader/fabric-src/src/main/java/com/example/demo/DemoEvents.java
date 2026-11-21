package com.example.demo;

import net.fabricmc.fabric.api.event.lifecycle.v1.ServerLifecycleEvents;

public final class DemoEvents {
    static void register() {
        ServerLifecycleEvents.SERVER_STARTED.register(server -> {});
    }
}
