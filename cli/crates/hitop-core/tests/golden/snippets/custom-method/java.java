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
            .uri(URI.create("https://cdn.example.test/assets/app.js"))
            .header("X-Tag", "b")
            .header("Authorization", "Basic " + Base64.getEncoder().encodeToString("svc:".getBytes(StandardCharsets.UTF_8)))
            .method("PURGE", HttpRequest.BodyPublishers.noBody())
            .build();
        HttpResponse<String> response = client.send(request, HttpResponse.BodyHandlers.ofString());
        System.out.println(response.statusCode());
        System.out.println(response.body());
    }
}
