// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Check12, ChevronDown16 } from '@mysten/icons';
import * as Select from '@radix-ui/react-select';
import clsx from 'clsx';

import { Text } from './Text';

export type ListboxSelectPros<T extends string = string> = {
	value: T;
	options: readonly T[];
	onSelect: (value: T) => void;
};

export function ListboxSelect<T extends string>({
	value,
	options,
	onSelect,
}: ListboxSelectPros<T>) {
	return (
		<Select.Root value={value} onValueChange={onSelect}>
			<div className="relative">
				<Select.Trigger className="group flex w-full flex-nowrap items-center gap-1 overflow-hidden text-hero-dark transition-all hover:text-hero-darkest">
					<Text variant="body/semibold">{value}</Text>
					<ChevronDown16
						className="text-gray-400 pointer-events-none h-4 w-4 text-steel transition-all group-hover:text-steel-dark"
						aria-hidden="true"
					/>
				</Select.Trigger>
				<Select.Content
					className={clsx(
						'pt-1',
						'data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95',
					)}
				>
					<Select.Viewport className="max-h-60 w-max max-w-xs rounded-lg bg-white p-2 shadow">
						<Select.Group>
							{options.map((option, index) => (
								<Select.Item
									key={index}
									className="flex flex-1 cursor-pointer flex-nowrap items-center gap-4 rounded-sm p-2 outline-none hover:bg-sui-light/40"
									value={option}
								>
									<Select.ItemText className="flex-1">
										<Text
											variant="caption/medium"
											color={value === option ? 'steel-darker' : 'steel-dark'}
											truncate
										>
											{option}
										</Text>
									</Select.ItemText>
									<Select.ItemIndicator>
										<Check12 className="h-4 w-4 text-steel-darker" />
									</Select.ItemIndicator>
								</Select.Item>
							))}
						</Select.Group>
					</Select.Viewport>
				</Select.Content>
			</div>
		</Select.Root>
	);
}
