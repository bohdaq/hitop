const testCollections = [
  {
    "id": 1,
    "name": "Default",
    "requests": [
      {
        "id": 1762811119201000,
        "name": "New2",
        "url": "${hostname}/todos/1",
        "method": "GET",
        "headers": [
          {
            "name": "",
            "value": ""
          }
        ],
        "body": "",
        "preRequestScript": "",
        "postRequestScript": "const id = response.userId + 1;\nsetContext('id', id);"
      },
      {
        "id": 1762815840441000,
        "name": "New3",
        "url": "${hostname}/todos/1",
        "method": "GET",
        "headers": [
          {
            "name": "",
            "value": ""
          }
        ],
        "body": "",
        "preRequestScript": "const hostname = getVariable('hostname');\nconst id = getContext('id');\nlet url = hostname + '/todos/' + id;\nsetUrl(url)",
        "postRequestScript": ""
      }
    ],
    "variables": {
      "hostname": "https://jsonplaceholder.typicode.com"
    }
  }
];

describe('Test Collections Data', () => {
  test('test data is loaded correctly', () => {
    expect(testCollections).toBeDefined();
    expect(testCollections).toHaveLength(1);
  });

  test('collection has correct name', () => {
    expect(testCollections[0].name).toBe('Default');
  });

  test('collection has requests', () => {
    expect(testCollections[0].requests).toBeDefined();
    expect(testCollections[0].requests.length).toBeGreaterThan(0);
  });

  test('collection has variables', () => {
    expect(testCollections[0].variables).toBeDefined();
    expect(Object.keys(testCollections[0].variables).length).toBeGreaterThan(0);
  });
});

describe('Collection Variables', () => {
  test('collection contains hostname variable', () => {
    const collection = testCollections[0];
    expect(collection.variables).toHaveProperty('hostname');
    expect(collection.variables.hostname).toBe('https://jsonplaceholder.typicode.com');
  });

  test('variables can be used in request URL', () => {
    const request = testCollections[0].requests[0];
    expect(request.url).toContain('${hostname}');
  });
});

describe('Request Scripts', () => {
  test('request has post-request script', () => {
    const request = testCollections[0].requests[0];
    expect(request.postRequestScript).toBeTruthy();
    expect(request.postRequestScript).toContain('setContext');
  });

  test('request has pre-request script', () => {
    const request = testCollections[0].requests[1];
    expect(request.preRequestScript).toBeTruthy();
    expect(request.preRequestScript).toContain('getVariable');
    expect(request.preRequestScript).toContain('getContext');
  });

  test('pre-request script uses variables and context', () => {
    const request = testCollections[0].requests[1];
    expect(request.preRequestScript).toContain("getVariable('hostname')");
    expect(request.preRequestScript).toContain("getContext('id')");
    expect(request.preRequestScript).toContain('setUrl');
  });

  test('post-request script sets context from response', () => {
    const request = testCollections[0].requests[0];
    expect(request.postRequestScript).toContain('response.userId');
    expect(request.postRequestScript).toContain("setContext('id'");
  });
});

describe('Collection Structure', () => {
  test('collection has correct structure', () => {
    const collection = testCollections[0];
    expect(collection).toHaveProperty('id');
    expect(collection).toHaveProperty('name');
    expect(collection).toHaveProperty('requests');
    expect(collection).toHaveProperty('variables');
  });

  test('requests have correct structure', () => {
    const request = testCollections[0].requests[0];
    expect(request).toHaveProperty('id');
    expect(request).toHaveProperty('name');
    expect(request).toHaveProperty('url');
    expect(request).toHaveProperty('method');
    expect(request).toHaveProperty('headers');
    expect(request).toHaveProperty('body');
    expect(request).toHaveProperty('preRequestScript');
    expect(request).toHaveProperty('postRequestScript');
  });

  test('collection contains multiple requests', () => {
    const collection = testCollections[0];
    expect(collection.requests).toHaveLength(2);
  });

  test('requests have unique IDs', () => {
    const collection = testCollections[0];
    const ids = collection.requests.map(r => r.id);
    const uniqueIds = new Set(ids);
    expect(uniqueIds.size).toBe(ids.length);
  });
});

describe('Request Methods', () => {
  test('all requests use GET method', () => {
    const collection = testCollections[0];
    collection.requests.forEach(request => {
      expect(request.method).toBe('GET');
    });
  });

  test('requests target correct endpoint', () => {
    const collection = testCollections[0];
    collection.requests.forEach(request => {
      expect(request.url).toContain('/todos/');
    });
  });
});
