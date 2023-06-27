// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type StoryObj, type Meta } from '@storybook/react';
import { useState } from 'react';

import { RadioGroup, RadioOption } from '~/ui/Radio';

export default {
	component: RadioGroup,
} as Meta;

const groups = [
	{
		value: '1',
		label: 'label 1',
		description: 'description 1',
	},
	{
		value: '2',
		label: 'label 2',
		description: 'description 2',
	},
	{
		value: '3',
		label: 'label 3',
		description: 'description 3',
	},
];

export const Default: StoryObj = {
	render: () => {
		const [selected, setSelected] = useState(groups[0].value);

		return (
			<div>
				<RadioGroup
					className="flex"
					value={selected}
					onValueChange={setSelected}
					aria-label="Default radio group"
				>
					{groups.map((group) => (
						<RadioOption key={group.label} value={group.value} label={group.label} />
					))}
				</RadioGroup>
			</div>
		);
	},
};
