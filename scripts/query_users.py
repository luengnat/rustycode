#!/usr/bin/env python3
"""
Script to connect to SQLite database and output all users as JSON.
"""

import sqlite3
import json
import sys
from pathlib import Path


def get_users_as_json(db_path="/tmp/test.db"):
    """
    Connect to SQLite database and retrieve all users.
    
    Args:
        db_path: Path to the SQLite database file
        
    Returns:
        JSON string containing all users
    """
    # Check if database exists
    if not Path(db_path).exists():
        return json.dumps({"error": f"Database not found at {db_path}"})
    
    try:
        # Connect to the database
        conn = sqlite3.connect(db_path)
        conn.row_factory = sqlite3.Row  # Enable column access by name
        cursor = conn.cursor()
        
        # Query all users (assuming a 'users' table)
        cursor.execute("SELECT * FROM users")
        
        # Fetch all rows and convert to list of dictionaries
        users = [dict(row) for row in cursor.fetchall()]
        
        # Close the connection
        conn.close()
        
        # Return as JSON
        return json.dumps(users, indent=2)
        
    except sqlite3.Error as e:
        return json.dumps({"error": f"Database error: {str(e)}"})
    except Exception as e:
        return json.dumps({"error": f"Unexpected error: {str(e)}"})


if __name__ == "__main__":
    # Allow passing database path as command line argument
    db_path = sys.argv[1] if len(sys.argv) > 1 else "/tmp/test.db"
    
    # Get and print users as JSON
    print(get_users_as_json(db_path))
