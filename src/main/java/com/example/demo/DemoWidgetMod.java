package com.example.demo;


public class DemoWidgetMod {
    public static final String MOD_ID = "demowidget";

    public void onInitialize() {
        System.out.println("[demowidget] " + greet());
    }

    public String greet() {
        return "Demo Widget loaded";
    }
}
