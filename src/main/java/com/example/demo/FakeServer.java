package com.example.demo;

import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.nio.file.Files;
import java.nio.file.Path;

/** Mimics a dedicated server: refuses to start without eula=true, then waits for "stop". */
public class FakeServer {
    public static void main(String[] args) throws Exception {
        if (!Files.readString(Path.of("eula.txt")).contains("eula=true")) {
            System.out.println("You need to agree to the EULA in order to run the server.");
            System.exit(1);
        }
        System.out.println("Starting demo server, widget size " + new DemoWidget().size());
        System.out.println("Done (0.1s)! For help, type \"help\"");
        BufferedReader in = new BufferedReader(new InputStreamReader(System.in));
        String line;
        while ((line = in.readLine()) != null) {
            if (line.trim().equals("stop")) {
                System.out.println("Stopping the server");
                return;
            }
        }
    }
}
