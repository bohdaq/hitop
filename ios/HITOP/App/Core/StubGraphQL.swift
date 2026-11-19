import Foundation

/// The shop-api's GraphQL schema as an introspection answer, for the stub (UI tests, screenshots)
/// and the unit tests.
enum StubGraphQL {
    private static func named(_ name: String, _ kind: String = "OBJECT") -> JSONAny {
        .object(["kind": .string(kind), "name": .string(name), "ofType": .null])
    }

    private static func nonNull(_ t: JSONAny) -> JSONAny { .object(["kind": .string("NON_NULL"), "name": .null, "ofType": t]) }
    private static func list(_ t: JSONAny) -> JSONAny { .object(["kind": .string("LIST"), "name": .null, "ofType": t]) }
    private static func scalar(_ n: String) -> JSONAny { named(n, "SCALAR") }

    private static func field(_ name: String, _ type: JSONAny, args: [(String, JSONAny)] = [], description: String? = nil) -> JSONAny {
        .object(["name": .string(name), "description": description.map(JSONAny.string) ?? .null, "isDeprecated": .bool(false),
                 "type": type, "args": .array(args.map { .object(["name": .string($0.0), "type": $0.1]) })])
    }

    private static func object(_ name: String, _ fields: [JSONAny], description: String? = nil) -> JSONAny {
        .object(["kind": .string("OBJECT"), "name": .string(name), "description": description.map(JSONAny.string) ?? .null,
                 "fields": .array(fields), "inputFields": .null, "enumValues": .null, "possibleTypes": .null])
    }

    static var schema: JSONAny {
        let product = named("Product")
        let types: [JSONAny] = [
            object("Query", [
                field("products", nonNull(list(nonNull(product))), args: [("first", scalar("Int")), ("category", scalar("ID"))], description: "Products in the catalog, newest first."),
                field("product", product, args: [("sku", nonNull(scalar("ID")))]),
                field("customer", named("Customer"), args: [("id", nonNull(scalar("ID")))]),
                field("orders", nonNull(list(nonNull(named("Order")))), args: [("status", named("OrderStatus", "ENUM"))]),
            ]),
            object("Mutation", [
                field("createOrder", nonNull(named("Order")), args: [("input", nonNull(named("OrderInput", "INPUT_OBJECT")))]),
                field("cancelOrder", named("Order"), args: [("id", nonNull(scalar("ID")))]),
            ]),
            object("Product", [
                field("sku", nonNull(scalar("ID"))), field("name", nonNull(scalar("String"))), field("price", nonNull(scalar("Int"))),
                field("stock", scalar("Int")), field("category", named("Category")),
            ], description: "Something the shop sells."),
            object("Category", [field("id", nonNull(scalar("ID"))), field("name", nonNull(scalar("String")))]),
            object("Customer", [
                field("id", nonNull(scalar("ID"))), field("name", nonNull(scalar("String"))), field("tier", scalar("String")),
                field("orders", nonNull(list(nonNull(named("Order"))))),
            ]),
            object("Order", [
                field("id", nonNull(scalar("ID"))), field("status", nonNull(named("OrderStatus", "ENUM"))), field("total", nonNull(scalar("Int"))),
                field("items", nonNull(list(nonNull(named("OrderItem"))))),
            ]),
            object("OrderItem", [field("sku", nonNull(scalar("ID"))), field("qty", nonNull(scalar("Int"))), field("product", named("Product"))]),
            .object(["kind": .string("ENUM"), "name": .string("OrderStatus"), "description": .null, "fields": .null, "inputFields": .null,
                     "enumValues": .array(["PENDING_PAYMENT", "PAID", "SHIPPED", "CANCELLED"].map { .object(["name": .string($0)]) }), "possibleTypes": .null]),
            .object(["kind": .string("INPUT_OBJECT"), "name": .string("OrderInput"), "description": .null, "fields": .null,
                     "inputFields": .array([field("customerId", nonNull(scalar("ID"))), field("skus", nonNull(list(nonNull(scalar("ID")))))]),
                     "enumValues": .null, "possibleTypes": .null]),
            object("__Schema", []),
        ] + ["ID", "String", "Int", "Boolean"].map { .object(["kind": .string("SCALAR"), "name": .string($0), "description": .null, "fields": .null]) }
        return .object(["data": .object(["__schema": .object([
            "queryType": .object(["name": .string("Query")]), "mutationType": .object(["name": .string("Mutation")]),
            "subscriptionType": .null, "types": .array(types),
        ])])])
    }

    static var introspection: String { String(decoding: schema.canonical, as: UTF8.self) }
}
