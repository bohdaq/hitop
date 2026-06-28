//
//  Item.swift
//  HiTop
//
//  Created by Bohdan Tsap on 29.06.2026.
//

import Foundation
import SwiftData

@Model
final class Item {
    var timestamp: Date
    
    init(timestamp: Date) {
        self.timestamp = timestamp
    }
}
