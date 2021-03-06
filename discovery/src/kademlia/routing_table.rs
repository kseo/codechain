use std::cmp;
use std::collections::{BTreeSet, HashMap, VecDeque};

use cnetwork::SocketAddr;

use super::contact::Contact;
use super::NodeId;

pub struct RoutingTable {
    local_id: NodeId,
    buckets: HashMap<usize, Bucket>,
    bucket_size: u8,
}

impl RoutingTable {
    pub fn new(local_id: NodeId, bucket_size: u8) -> Self {
        const CAPACITY: usize = 8;
        RoutingTable {
            local_id,
            buckets: HashMap::with_capacity(CAPACITY),
            bucket_size,
        }
    }

    pub fn local_id(&self) -> NodeId {
        self.local_id
    }

    pub fn touch_contact(&mut self, contact: Contact) -> Option<&Contact> {
        let index = contact.log2_distance(&self.local_id);
        // FIXME: Decide the maximum distance to contact.
        if index == 0 {
            return None
        }
        let bucket = self.add_bucket(index);
        bucket.touch_contact(contact)
    }

    #[allow(dead_code)]
    pub fn remove_contact(&mut self, contact: &Contact) -> Option<&Contact> {
        let index = contact.log2_distance(&self.local_id);
        if index == 0 {
            return None
        }

        let bucket = self.buckets.get_mut(&index);
        bucket.and_then(|bucket| bucket.remove_contact(contact))
    }

    fn add_bucket(&mut self, index: usize) -> &mut Bucket {
        self.buckets.entry(index).or_insert(Bucket::new(self.bucket_size))
    }

    pub fn get_closest_contacts(&self, target: &NodeId, result_limit: u8) -> Vec<Contact> {
        let contacts = self.get_contacts_in_distance_order(target);
        contacts
            .into_iter()
            .take(cmp::min(result_limit, self.bucket_size) as usize)
            .map(|item| {
                debug_assert_ne!(target, &item.contact.id());
                debug_assert_ne!(self.local_id, item.contact.id());
                item.contact
            })
            .collect()
    }

    fn get_contacts_in_distance_order(&self, target: &NodeId) -> BTreeSet<ContactWithDistance> {
        let mut result = BTreeSet::new();
        let mut max_distance = 0;
        for (_, bucket) in self.buckets.iter() {
            for i in 0..self.bucket_size {
                let contact = bucket.contacts.get(i as usize);
                if contact.is_none() {
                    break
                }

                let contact = contact.unwrap();

                if target == &contact.id() {
                    continue
                }

                let item = ContactWithDistance::new(contact, target);
                if max_distance < item.distance {
                    if (self.bucket_size as usize) <= result.len() {
                        // FIXME: Remove the last item to guarantee the maximum size of return value.
                        continue
                    }
                    max_distance = item.distance;
                }
                result.insert(item);
            }
        }
        result
    }

    pub fn contains(&self, contact: &Contact) -> bool {
        let index = contact.log2_distance(&self.local_id);
        if index == 0 {
            return false
        }

        let bucket = self.buckets.get(&index);
        match bucket.map(|bucket| bucket.contains(contact)) {
            None => false,
            Some(has) => has,
        }
    }

    pub fn conflicts(&self, contact: &Contact) -> bool {
        let index = contact.log2_distance(&self.local_id);
        if index == 0 {
            return true
        }
        let bucket = self.buckets.get(&index);
        if let Some(bucket) = bucket {
            bucket.conflicts(contact)
        } else {
            false
        }
    }

    pub fn cleanup(&mut self) {
        self.buckets.retain(|_, bucket| !bucket.is_empty());
    }

    pub fn distances(&self) -> Vec<usize> {
        self.buckets.keys().cloned().collect()
    }

    pub fn get_contacts_with_distance(&self, distance: usize) -> Vec<Contact> {
        self.buckets.get(&distance).map(|bucket| Vec::from(bucket.contacts.clone())).unwrap_or(vec![])
    }

    pub fn remove_address(&mut self, address: &SocketAddr) {
        for bucket in self.buckets.values_mut() {
            bucket.remove_address(&address);
        }
    }

    pub fn len(&self) -> usize {
        self.buckets.values().map(|bucket| bucket.contacts.len()).sum()
    }
}


struct Bucket {
    contacts: VecDeque<Contact>,
    bucket_size: u8,
}

impl Bucket {
    pub fn new(bucket_size: u8) -> Self {
        Bucket {
            contacts: VecDeque::new(),
            bucket_size,
        }
    }

    pub fn touch_contact(&mut self, contact: Contact) -> Option<&Contact> {
        self.remove_contact(&contact);
        if !self.conflicts(&contact) {
            self.contacts.push_back(contact);
        }
        self.head_if_full()
    }


    pub fn remove_contact(&mut self, contact: &Contact) -> Option<&Contact> {
        self.contacts.retain(|old_contact| old_contact != contact);
        self.head_if_full()
    }

    fn head_if_full(&self) -> Option<&Contact> {
        if self.contacts.len() > self.bucket_size as usize {
            self.contacts.front()
        } else {
            None
        }
    }

    pub fn is_empty(&self) -> bool {
        self.contacts.is_empty()
    }

    fn contains(&self, contact: &Contact) -> bool {
        self.contacts.contains(contact)
    }

    pub fn conflicts(&self, contact: &Contact) -> bool {
        self.contacts
            .iter()
            .find(|old_contact| old_contact.id() == contact.id() && old_contact.addr() != contact.addr())
            .is_some()
    }

    fn remove_address(&mut self, address: &SocketAddr) {
        self.contacts.retain(|contact| contact.addr() != address);
    }
}


#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ContactWithDistance {
    distance: usize,
    contact: Contact,
}

impl ContactWithDistance {
    pub fn new(contact: &Contact, target: &NodeId) -> Self {
        ContactWithDistance {
            distance: contact.log2_distance(&target),
            contact: contact.clone(),
        }
    }
}


#[cfg(test)]
mod tests {
    use super::super::contact::Contact;
    use super::RoutingTable;

    const IDS: [&str; 18] = [
        "0000000000000000000000000000000000000000000000000000000000000000",
        "0000000000000000000000000000000000000000000000000000000000000001",
        "0000000000000000000000000000000000000000000000000000000000000002",
        "0000000000000000000000000000000000000000000000000000000000000003",
        "0000000000000000000000000000000000000000000000000000000000000004",
        "0000000000000000000000000000000000000000000000000000000000000005",
        "0000000000000000000000000000000000000000000000000000000000000006",
        "0000000000000000000000000000000000000000000000000000000000000007",
        "0000000000000000000000000000000000000000000000000000000000000008",
        "0000000000000000000000000000000000000000000000000000000000000009",
        "000000000000000000000000000000000000000000000000000000000000000a",
        "000000000000000000000000000000000000000000000000000000000000000b",
        "000000000000000000000000000000000000000000000000000000000000000c",
        "000000000000000000000000000000000000000000000000000000000000000d",
        "000000000000000000000000000000000000000000000000000000000000000e",
        "000000000000000000000000000000000000000000000000000000000000000f",
        "0000000000000000000000000000000000000000000000000000000000000010",
        "0000000000000000000000000000000000000000000000000000000000000011",
    ];

    fn get_contact(distance_from_zero: usize) -> Contact {
        Contact::from_hash(IDS[distance_from_zero])
    }

    fn get_contact_with_address(distance_from_zero: usize, a: u8, b: u8, c: u8, d: u8, port: u16) -> Contact {
        Contact::from_hash_with_addr(IDS[distance_from_zero], a, b, c, d, port)
    }

    fn init_routing_table(bucket_size: u8, local_index: usize) -> RoutingTable {
        let local_id = get_contact(local_index).id();
        let mut routing_table = RoutingTable::new(local_id, bucket_size);

        for i in 0..IDS.len() {
            if i == local_index {
                continue
            }
            routing_table.touch_contact(get_contact(i));
        }
        routing_table
    }

    #[test]
    fn test_size_of_closest_contacts_is_not_larger_than_bucket_size() {
        const BUCKET_SIZE: u8 = 5;
        let routing_table = init_routing_table(BUCKET_SIZE, 0);

        let closest_contacts = routing_table.get_closest_contacts(&get_contact(4).id(), BUCKET_SIZE);
        assert!(closest_contacts.len() <= (BUCKET_SIZE as usize));
    }

    #[test]
    fn test_closest_contacts_1() {
        const BUCKET_SIZE: u8 = 5;
        let routing_table = init_routing_table(BUCKET_SIZE, 0);

        let closest_contacts = routing_table.get_closest_contacts(&get_contact(4).id(), BUCKET_SIZE);
        assert_eq!(BUCKET_SIZE as usize, closest_contacts.len());
        assert_eq!(get_contact(5), closest_contacts[0]);
        assert_eq!(get_contact(6), closest_contacts[1]);
        assert_eq!(get_contact(7), closest_contacts[2]);
        assert_eq!(get_contact(1), closest_contacts[3]);
        assert_eq!(get_contact(2), closest_contacts[4]);
    }

    #[test]
    fn test_closest_contacts_2() {
        const BUCKET_SIZE: u8 = 5;
        let routing_table = init_routing_table(BUCKET_SIZE, 0);

        let closest_contacts = routing_table.get_closest_contacts(&get_contact(3).id(), BUCKET_SIZE);
        assert_eq!(BUCKET_SIZE as usize, closest_contacts.len());
        assert_eq!(get_contact(2), closest_contacts[0]);
        assert_eq!(get_contact(1), closest_contacts[1]);
        assert_eq!(get_contact(4), closest_contacts[2]);
        assert_eq!(get_contact(5), closest_contacts[3]);
        assert_eq!(get_contact(6), closest_contacts[4]);
    }

    #[test]
    fn test_closest_contacts_must_not_contain_target() {
        use std::u8;
        debug_assert!(IDS.len() <= (u8::MAX as usize));
        let bucket_size = IDS.len() as u8;
        let routing_table = init_routing_table(bucket_size, 0);

        const TARGET_INDEX: usize = 3;
        let closest_contacts = routing_table.get_closest_contacts(&get_contact(TARGET_INDEX).id(), bucket_size);
        assert!(!closest_contacts.contains(&get_contact(TARGET_INDEX)));
        assert!(2 <= IDS.len());
        let number_of_contacts_except_local = IDS.len() - 1;
        let number_of_contacts_except_local_and_target = number_of_contacts_except_local - 1;
        assert_eq!(number_of_contacts_except_local_and_target, closest_contacts.len());
    }

    #[test]
    fn test_add_contact_fails_when_there_is_duplicated_id_with_diffrent_address() {
        use std::u8;
        debug_assert!(IDS.len() <= (u8::MAX as usize));
        let bucket_size = IDS.len() as u8;
        let mut routing_table = init_routing_table(bucket_size, 0);

        let new_contact = get_contact_with_address(4, 127, 0, 0, 1, 3485);
        routing_table.touch_contact(new_contact.clone());
        let closest_contacts = routing_table.get_closest_contacts(&new_contact.id(), bucket_size);
        assert!(!closest_contacts.contains(&new_contact));
    }

    #[test]
    fn test_closest_contacts_must_not_contain_removed() {
        use std::u8;
        debug_assert!(IDS.len() <= (u8::MAX as usize));
        let bucket_size = IDS.len() as u8;
        let mut routing_table = init_routing_table(bucket_size, 0);

        const KILLED_INDEX: usize = 4;
        routing_table.remove_contact(&get_contact(KILLED_INDEX));

        const TARGET_INDEX: usize = 5;
        let closest_contacts = routing_table.get_closest_contacts(&get_contact(TARGET_INDEX).id(), bucket_size);
        assert!(!closest_contacts.contains(&get_contact(KILLED_INDEX)));
    }

    #[test]
    fn test_closest_contacts_takes_the_limit() {
        use std::u8;
        debug_assert!(IDS.len() <= (u8::MAX as usize));
        let bucket_size = IDS.len() as u8;
        let routing_table = init_routing_table(bucket_size, 0);

        const TARGET_INDEX: usize = 5;

        const RESULT_LIMIT3: u8 = 3;
        let closest_contacts = routing_table.get_closest_contacts(&get_contact(TARGET_INDEX).id(), RESULT_LIMIT3);
        assert_eq!(RESULT_LIMIT3 as usize, closest_contacts.len());

        const RESULT_LIMIT2: u8 = 2;
        let closest_contacts = routing_table.get_closest_contacts(&get_contact(TARGET_INDEX).id(), RESULT_LIMIT2);
        assert_eq!(RESULT_LIMIT2 as usize, closest_contacts.len());

        const RESULT_LIMIT7: u8 = 7;
        let closest_contacts = routing_table.get_closest_contacts(&get_contact(TARGET_INDEX).id(), RESULT_LIMIT7);
        assert_eq!(RESULT_LIMIT7 as usize, closest_contacts.len());

        const RESULT_LIMIT5: u8 = 5;
        let closest_contacts = routing_table.get_closest_contacts(&get_contact(TARGET_INDEX).id(), RESULT_LIMIT5);
        assert_eq!(RESULT_LIMIT5 as usize, closest_contacts.len());
    }

    #[test]
    fn test_conflicts_if_different_address_with_same_id() {
        use std::u8;
        debug_assert!(IDS.len() <= (u8::MAX as usize));
        let bucket_size = IDS.len() as u8;
        let routing_table = init_routing_table(bucket_size, 0);

        let new_contact = get_contact_with_address(4, 127, 0, 0, 1, 3485);
        assert!(routing_table.conflicts(&new_contact));
    }

    #[test]
    fn test_same_id_and_address_does_not_conflict() {
        use std::u8;
        debug_assert!(IDS.len() <= (u8::MAX as usize));
        let bucket_size = IDS.len() as u8;
        let routing_table = init_routing_table(bucket_size, 0);

        let new_contact = get_contact(4);
        assert!(!routing_table.conflicts(&new_contact));
    }

    #[test]
    fn test_get_contacts_with_distance() {
        const BUCKET_SIZE: u8 = 5;
        let routing_table = init_routing_table(BUCKET_SIZE, 0);

        assert_eq!(1, routing_table.get_contacts_with_distance(1).len());
        assert_eq!(2, routing_table.get_contacts_with_distance(2).len());
        assert_eq!(4, routing_table.get_contacts_with_distance(3).len());
        assert_eq!(8, routing_table.get_contacts_with_distance(4).len());
        assert_eq!(2, routing_table.get_contacts_with_distance(5).len());
    }
}
