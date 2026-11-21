package com.example.demo;

import net.neoforged.fml.common.Mod;

@Mod(DemoWidgetMod.MOD_ID)
public class DemoWidgetMod {
    public static final String MOD_ID = "demowidget";

    public DemoWidgetMod() {
        System.out.println("Demo Widget loaded");
    }
}
