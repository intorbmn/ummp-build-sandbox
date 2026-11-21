package com.example.demo;

import net.fabricmc.api.ModInitializer;

public class DemoWidgetMod implements ModInitializer, DemoApi {
    public static final String MOD_ID = "demowidget";

    @Override
    public void onInitialize() {
        System.out.println("Demo Widget loaded");
    }
}
