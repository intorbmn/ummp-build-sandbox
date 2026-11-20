package com.example.demo;

import net.fabricmc.api.ModInitializer;

public class DemoWidgetMod implements ModInitializer {
    public static final String MOD_ID = "demowidget";

    @Override
    public void onInitialize() {
        System.out.println("[demowidget] " + greet());
    }

    public String greet() {
        return "Demo Widget loaded";
    }
}
