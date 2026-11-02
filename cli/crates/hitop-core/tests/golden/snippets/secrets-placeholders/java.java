import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;

public class Main {
    public static void main(String[] args) throws Exception {
        HttpClient client = HttpClient.newHttpClient();
        HttpRequest request = HttpRequest.newBuilder()
            .uri(URI.create("https://api.example.test/tenants/{{tenant}}/note"))
            .header("X-Trace", "t-1")
            .header("Authorization", "Bearer {{token}}")
            .header("Content-Type", "text/plain")
            .method("PATCH", HttpRequest.BodyPublishers.ofString("plain text\twith a tab"))
            .build();
        HttpResponse<String> response = client.send(request, HttpResponse.BodyHandlers.ofString());
        System.out.println(response.statusCode());
        System.out.println(response.body());
    }
}
