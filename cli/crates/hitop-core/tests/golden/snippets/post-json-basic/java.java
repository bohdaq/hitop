import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.nio.charset.StandardCharsets;
import java.util.Base64;

public class Main {
    public static void main(String[] args) throws Exception {
        HttpClient client = HttpClient.newHttpClient();
        HttpRequest request = HttpRequest.newBuilder()
            .uri(URI.create("https://api.example.test/items"))
            .header("Content-Type", "application/json")
            .header("Authorization", "Basic " + Base64.getEncoder().encodeToString("ada:p'w\"d".getBytes(StandardCharsets.UTF_8)))
            .method("POST", HttpRequest.BodyPublishers.ofString("{\n  \"name\": \"it's \\\"quoted\\\"\",\n  \"path\": \"C:\\\\tmp\",\n  \"shell\": \"$HOME `id`\",\n  \"city\": \"Kraków\"\n}"))
            .build();
        HttpResponse<String> response = client.send(request, HttpResponse.BodyHandlers.ofString());
        System.out.println(response.statusCode());
        System.out.println(response.body());
    }
}
